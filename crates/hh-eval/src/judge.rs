//! `judge` — the eval-plane judge admission and judged-detector emission
//! (spec §5h.2 + §5f.4; AC-R-2.9.2-14; ADR-0047 D3/(c)(ii), ADR-0115…0117;
//! ticket S4.15, R-2.9.2²).
//!
//! The deterministic oracles of [`crate::oracle`] refuse `judge` outright
//! (`OracleFailure::NondeterministicClass`); this module is the boundary a
//! judge-class [`OracleDeclaration`] must pass *before* its verdict may land
//! as a [`MetricValue`]. Admission enforces the AC-R-2.9.2-14 halves the
//! declaration schema cannot:
//!
//! - **independence** — a judge declared with the beneficiary's snapshot is
//!   refused `JudgeNotIndependent`; `same_context` judges are refused
//!   outright (same-context critics are not critics). The remaining axes
//!   bind per use: `gate` carries the ADR-0116 D2 minimums (`context =
//!   fresh`, `provenance ≠ model_authored_rubric`, `capability ≠
//!   shared_mutable`, `optimization ≠ in_loop_unbounded`); a `report` use
//!   records the vector on the emission, never widens.
//! - **calibration** — `use = gate` without an `active` calibration emits
//!   the typed `n/a{no_detector}` cell (never a refusal-shaped value, never
//!   the judged verdict); `use = report` without one emits `exploratory`
//!   (report-only evidence, never gate-admissible).
//! - **charging** — every judge call is `charged_to = instrument`; a
//!   declaration claiming subject spend is refused `JudgeNotInstrument`
//!   (re-checked here beside `OracleDeclaration::validate`).
//! - **evidence** — `evidence_out ⊆ {model_io, human_attestation}` is
//!   `ModelClaimOnly`: a judge whose output is only the beneficiary's own
//!   claims/attestations produces no evidence.
//!
//! Every emission stamps `detector = judged` on the `MetricValue` — the
//! judged execution-alignment metric (`execution_alignment_failure_rate_
//! judged`) and the deterministic fold (`execution_alignment_failure_rate`)
//! are distinct declarations and are never merged (CF-483; ADR-0114).

use std::collections::BTreeSet;

use hh_ontology::compliance::{Detector, MetricDeclaration, NaReason};
use hh_ontology::eval::{
    ChargedTo, EvidenceKind, MetricValue, MetricValueKind, OracleClass, OracleDeclaration,
    OracleError,
};
use hh_verification::critics::IndependenceVector;
use hh_verification::vocab::{
    CalibrationStatus, CapabilityIndependence, ContextIndependence, CriticUse, Optimization,
    ProvenanceIndependence, SnapshotIndependence,
};

/// `admit_judge`/`emit_judged` refusals (typed — never a warning).
#[derive(Debug, Clone, PartialEq)]
pub enum JudgeError {
    /// The declaration's class is not `judge` (the deterministic classes
    /// route through `run_oracle`; `teacher`/`human` land with their own
    /// tickets).
    NotAJudge {
        /// The declared class.
        class: String,
    },
    /// The declaration failed `OracleDeclaration::validate`.
    InvalidDeclaration(OracleError),
    /// The judge's evidence is only the beneficiary's own claims —
    /// `evidence_out ⊆ {model_io, human_attestation}` produces nothing a
    /// verdict may stand on (the `evidence_out` clause of ADR-0047 D3).
    ModelClaimOnly,
    /// The judge shares the beneficiary's snapshot (`same_snapshot`), or
    /// shares its context (`same_context`) — refused for every use at this
    /// boundary (AC-R-2.9.2-14 names the flat refusal; the critic plane's
    /// report-side recording of the weaker class does not apply to eval
    /// oracles).
    JudgeNotIndependent {
        /// The violated axis.
        axis: String,
    },
    /// The independence vector is below the use's minimum
    /// (`IndependenceBelowMinimum` — the ADR-0116 D2 gate floor).
    IndependenceBelowMinimum {
        /// The use.
        use_: String,
        /// The violated axis.
        axis: String,
    },
    /// The metric declaration does not admit a judged detector
    /// (`detector_classes_allowed` lacks `judged` — a judged value never
    /// lands on a deterministic-only metric, and the names never merge).
    DetectorNotAdmitted {
        /// The metric.
        metric: String,
    },
    /// The metric declaration does not admit the `judge` oracle class
    /// (`oracle_classes_allowed` lacks `judge` — ADR-0047 D1's bound).
    OracleNotAdmitted {
        /// The metric.
        metric: String,
    },
}

impl std::fmt::Display for JudgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JudgeError::NotAJudge { class } => write!(f, "NotAJudge({class})"),
            JudgeError::InvalidDeclaration(e) => write!(f, "InvalidDeclaration: {e:?}"),
            JudgeError::ModelClaimOnly => write!(f, "ModelClaimOnly"),
            JudgeError::JudgeNotIndependent { axis } => {
                write!(f, "JudgeNotIndependent: {axis}")
            }
            JudgeError::IndependenceBelowMinimum { use_, axis } => {
                write!(f, "IndependenceBelowMinimum({use_}): {axis}")
            }
            JudgeError::DetectorNotAdmitted { metric } => {
                write!(f, "DetectorNotAdmitted({metric})")
            }
            JudgeError::OracleNotAdmitted { metric } => {
                write!(f, "OracleNotAdmitted({metric})")
            }
        }
    }
}

impl std::error::Error for JudgeError {}

/// The judge admission's inputs — what the declaration cannot carry: the
/// judge's model-snapshot binding, the beneficiary's snapshots/families,
/// the independence vector the binding resolves to, the calibration the
/// `calibration_ref` currently resolves to, and the use the value feeds.
#[derive(Debug, Clone)]
pub struct JudgeContext {
    /// The judge's own model snapshot ref (`profile_binding[judge]`'s
    /// resolved snapshot — ADR-0121 D2).
    pub judge_snapshot: String,
    /// The judge's snapshot family, where the snapshot record declares one.
    pub judge_family: Option<String>,
    /// The beneficiary's model snapshot refs (the subject arm's
    /// `model_snapshots` — every coordinate the run's behaviour derives
    /// from).
    pub beneficiary_snapshots: BTreeSet<String>,
    /// The beneficiary snapshots' families.
    pub beneficiary_families: BTreeSet<String>,
    /// The judge's independence vector (mandatory — ADR-0116 D1).
    pub independence: IndependenceVector,
    /// The calibration the declaration's `calibration_ref` resolves to
    /// (`None` = no calibration record — `expired` and absent degrade
    /// identically: `n/a{no_detector}` for gate, `exploratory` for report).
    pub calibration: Option<CalibrationStatus>,
    /// The use the emitted value feeds (`report` | `gate`).
    pub use_: CriticUse,
}

/// The admission verdict — what a context-checked judge may emit.
#[derive(Debug, Clone, PartialEq)]
pub enum JudgeAdmission {
    /// Admitted — `exploratory` marks a report-use emission without active
    /// calibration (report-only evidence; never gate-admissible).
    Admitted {
        /// Report-only (uncalibrated `report` use — ADR-0047(c)(ii)).
        exploratory: bool,
    },
    /// A gating use without an active calibration — the cell renders
    /// `n/a{no_detector}`; the judged verdict is never emitted for gating.
    Unavailable {
        /// The typed n/a reason (`no_detector`).
        reason: NaReason,
    },
}

/// The snapshot axis is *derived* from the bound snapshots — never
/// declared (CC2: authority is conferred, never widened — a declaration
/// claiming `different_family` under a bound `same_family` overclaims).
fn derive_snapshot_axis(ctx: &JudgeContext) -> SnapshotIndependence {
    if ctx
        .beneficiary_snapshots
        .contains(ctx.judge_snapshot.as_str())
    {
        return SnapshotIndependence::SameSnapshot;
    }
    let same_family = ctx
        .judge_family
        .as_ref()
        .map(|f| ctx.beneficiary_families.contains(f))
        .unwrap_or(false);
    if same_family {
        SnapshotIndependence::DifferentSnapshotSameFamily
    } else {
        SnapshotIndependence::DifferentFamily
    }
}

/// Whether the declaration is a judge the boundary may consider —
/// declaration-level invariants re-checked beside `validate` (a caller
/// skipping schema validation still gets the refusals).
pub fn admit_judge(
    oracle: &OracleDeclaration,
    ctx: &JudgeContext,
) -> Result<JudgeAdmission, JudgeError> {
    if oracle.class != OracleClass::Judge {
        return Err(JudgeError::NotAJudge {
            class: oracle.class.as_str().to_string(),
        });
    }
    // The declaration's schema checks are re-run here — a registry row that
    // skipped validate() still meets the judge invariants at this boundary.
    if let Err(e) = oracle.validate() {
        return Err(JudgeError::InvalidDeclaration(e));
    }
    // `evidence_out` never only model-claim: a judge whose verdict grounds
    // only on the beneficiary's own I/O or attestations — or on nothing at
    // all — emits no evidence.
    let claim_only = oracle
        .evidence_out
        .iter()
        .all(|k| matches!(k, EvidenceKind::ModelIo | EvidenceKind::HumanAttestation));
    if claim_only {
        return Err(JudgeError::ModelClaimOnly);
    }

    // ── independence (AC-R-2.9.2-14) ───────────────────────────────────
    let snapshot_axis = derive_snapshot_axis(ctx);
    if snapshot_axis == SnapshotIndependence::SameSnapshot {
        return Err(JudgeError::JudgeNotIndependent {
            axis: "snapshot = same_snapshot".into(),
        });
    }
    // Same-context judges are not critics — refused for every use.
    if ctx.independence.context == ContextIndependence::SameContext {
        return Err(JudgeError::JudgeNotIndependent {
            axis: "context = same_context".into(),
        });
    }
    // A declared `different_family` under a derived `same_family` axis is an
    // inflated independence basis — refused, never recorded.
    if ctx.independence.snapshot == SnapshotIndependence::DifferentFamily
        && snapshot_axis == SnapshotIndependence::DifferentSnapshotSameFamily
    {
        return Err(JudgeError::JudgeNotIndependent {
            axis: "declared snapshot axis overclaims the derived one".into(),
        });
    }

    // ── the per-use minimums (ADR-0116 D2's gate floor) ────────────────
    if ctx.use_ == CriticUse::Gate {
        if ctx.independence.context != ContextIndependence::Fresh {
            return Err(JudgeError::IndependenceBelowMinimum {
                use_: "gate".into(),
                axis: "context ≠ fresh".into(),
            });
        }
        if ctx.independence.provenance == ProvenanceIndependence::ModelAuthoredRubric {
            return Err(JudgeError::IndependenceBelowMinimum {
                use_: "gate".into(),
                axis: "provenance = model_authored_rubric".into(),
            });
        }
        if ctx.independence.capability == CapabilityIndependence::SharedMutable {
            return Err(JudgeError::IndependenceBelowMinimum {
                use_: "gate".into(),
                axis: "capability = shared_mutable".into(),
            });
        }
        if ctx.independence.optimization == Optimization::InLoopUnbounded {
            return Err(JudgeError::IndependenceBelowMinimum {
                use_: "gate".into(),
                axis: "optimization = in_loop_unbounded".into(),
            });
        }
    }

    // ── calibration (ADR-0047(c)(ii); ADR-0117 D2) ─────────────────────
    let calibrated = ctx.calibration == Some(CalibrationStatus::Active);
    match ctx.use_ {
        CriticUse::Gate if !calibrated => Ok(JudgeAdmission::Unavailable {
            reason: NaReason::NoDetector,
        }),
        CriticUse::Report => Ok(JudgeAdmission::Admitted {
            exploratory: !calibrated,
        }),
        CriticUse::Gate => Ok(JudgeAdmission::Admitted { exploratory: false }),
    }
}

/// What an admitted judge call commits — the `MetricValue` row the
/// `measurement.metric.emitted` event carries plus the call's accounting
/// and independence stamps (the emission is the charged record).
#[derive(Debug, Clone, PartialEq)]
pub struct JudgeEmission {
    /// The emitted `MetricValue` (`detector = judged` always; `value =
    /// n/a{no_detector}` for an uncalibrated gate use — the verdict is
    /// never smuggled into a gating row).
    pub value: MetricValue,
    /// `exploratory` — report-only evidence (uncalibrated `report` use);
    /// never gate-admissible (ADR-0047(c)(ii)).
    pub exploratory: bool,
    /// The `independence_summary` the emission carries (ADR-0116 D1).
    pub independence_summary: String,
    /// Every judge call charges to `instrument` (AC-R-2.9.2-14).
    pub charged_to: ChargedTo,
    /// The oracle's `calibration_ref`, where declared (the emitted
    /// provenance — a `None` on an `exploratory`/`unavailable` emission is
    /// the honest absence, never a ref minted post-hoc).
    pub calibration_ref: Option<String>,
}

/// `emit_judged(metric, oracle, ctx, verdict)` — the judged-detector
/// emission: admission runs first (`admit_judge`), the metric declaration
/// must admit `detector = judged` and `oracle_class = judge`, and the
/// emitted `MetricValue` carries `detector = judged` under the oracle's
/// `oracle_id`. A gate use without active calibration emits
/// `n/a{no_detector}` — the `verdict` is discarded on that row (an
/// uncalibrated judge produces no gate evidence at all).
#[allow(clippy::too_many_arguments)] // the emission's members are the record's shape.
pub fn emit_judged(
    metric: &MetricDeclaration,
    oracle: &OracleDeclaration,
    ctx: &JudgeContext,
    verdict: MetricValueKind,
    applies_to: &str,
    confidence: Option<u64>,
    evidence_ref: Option<String>,
) -> Result<JudgeEmission, JudgeError> {
    if !metric.detector_classes_allowed.contains(&Detector::Judged) {
        return Err(JudgeError::DetectorNotAdmitted {
            metric: metric.name.clone(),
        });
    }
    if !metric.oracle_classes_allowed.contains(&OracleClass::Judge) {
        return Err(JudgeError::OracleNotAdmitted {
            metric: metric.name.clone(),
        });
    }
    let admission = admit_judge(oracle, ctx)?;

    let (value, exploratory) = match admission {
        JudgeAdmission::Unavailable { reason } => (MetricValueKind::Na(reason), false),
        JudgeAdmission::Admitted { exploratory } => (verdict, exploratory),
    };

    // The vector the verdict records carries the *derived* snapshot axis —
    // the context's declared vector is re-based so a `same_family`
    // derivation is never rendered `different_family` (CC2).
    let mut vector = ctx.independence.clone();
    vector.snapshot = derive_snapshot_axis(ctx);

    Ok(JudgeEmission {
        value: MetricValue {
            metric_ref: metric.name.clone(),
            value,
            applies_to: applies_to.to_string(),
            oracle_ref: oracle.oracle_id.clone(),
            detector: Detector::Judged,
            confidence,
            evidence_ref,
        },
        exploratory,
        independence_summary: vector.summary(),
        charged_to: ChargedTo::Instrument,
        calibration_ref: oracle.calibration_ref.clone(),
    })
}
