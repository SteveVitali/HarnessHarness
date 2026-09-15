//! Validity vs compliance as a measured pair (spec §2.6; ADR-0014).
//!
//! Harness **validity** (the artifact encodes a correct policy/fact) and **compliance** (the
//! beneficiary model notices, activates and follows it) are measured separately and never
//! reported as one number (§2.6). This module lands the C0/Stage-1 *shapes*:
//! - [`validity`] — three-valued, decided by a protocol-layer check that **never consults the
//!   beneficiary model**; structurally model-free (no `M`/configuration input) so the result is
//!   identical across configurations (the S-077 property; §2.6.1);
//! - the three [`ChainEvent`]s (`delivered` → `activated` → `followed`) with `detector`
//!   provenance (§2.6.2) — the *schema* only; emission is Stage 2/3;
//! - [`MetricDeclaration`] carrying `applies_to_classes`, `requires_observability`,
//!   `detector_classes_allowed` and the typed [`NaReason`] never coerced to 0 (§2.6.3; T-LCD-15).

use crate::participant::{Observability, ParticipantClass, ParticipantDescriptor};
use std::collections::BTreeSet;

/// The three-valued validity state (§2.6.1). `unknown` when no check exists — never guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Validity {
    /// A check decided valid.
    Valid,
    /// A check decided invalid.
    Invalid,
    /// No check exists (or it could not decide).
    Unknown,
}

/// The protocol-layer check `validity` consults (§2.6.1). None of these consults the beneficiary
/// model — the crux of the S-077 property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidityCheck {
    /// A version stamp comparison — valid iff the stamp is the expected version.
    VersionStamp { stamp: String, expected: String },
    /// An executable validator (`validates` edge) — its precomputed, model-free verdict.
    ExecutableValidator { passed: bool, check_ref: String },
    /// An external oracle verdict.
    ExternalOracle { valid: bool, check_ref: String },
    /// A declared validity interval — whether `at` falls inside it.
    DeclaredInterval { active_at: bool, check_ref: String },
    /// No check declared.
    None,
}

/// The outcome of [`validity`]: `{valid, invalid, unknown, check_ref}` (§2.6.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidityOutcome {
    /// The three-valued state.
    pub state: Validity,
    /// The check reference, when one existed.
    pub check_ref: Option<String>,
}

/// `validity(α, at) → {valid, invalid, unknown, check_ref}` (§2.9.1). Decided *only* by the
/// protocol-layer check; there is deliberately **no model or configuration parameter**, so the
/// result is identical across configurations (§2.6.1 invariant; ADR-0014). `at` is the event id
/// the interval check is evaluated at.
pub fn validity(check: &ValidityCheck, _at: &str) -> ValidityOutcome {
    match check {
        ValidityCheck::VersionStamp { stamp, expected } => ValidityOutcome {
            state: if stamp == expected {
                Validity::Valid
            } else {
                Validity::Invalid
            },
            check_ref: Some(format!("version:{expected}")),
        },
        ValidityCheck::ExecutableValidator { passed, check_ref } => ValidityOutcome {
            state: if *passed {
                Validity::Valid
            } else {
                Validity::Invalid
            },
            check_ref: Some(check_ref.clone()),
        },
        ValidityCheck::ExternalOracle { valid, check_ref } => ValidityOutcome {
            state: if *valid {
                Validity::Valid
            } else {
                Validity::Invalid
            },
            check_ref: Some(check_ref.clone()),
        },
        ValidityCheck::DeclaredInterval {
            active_at,
            check_ref,
        } => ValidityOutcome {
            state: if *active_at {
                Validity::Valid
            } else {
                Validity::Invalid
            },
            check_ref: Some(check_ref.clone()),
        },
        ValidityCheck::None => ValidityOutcome {
            state: Validity::Unknown,
            check_ref: None,
        },
    }
}

/// The detector provenance of a compliance signal (§2.6.2; CF-483; ADR-0110). One sum shared
/// with `Verdict.detector`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Detector {
    /// A deterministic detector (schema/predicate, handle-only expansion, retrieval injection …).
    Deterministic,
    /// A judged detector (enters at C2).
    Judged,
    /// A human detector.
    Human,
}

/// The kinds a harness artifact may take (§2.6.2). `activation_observable` is per-kind
/// (§2.9.3 Stage-1 scope).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtifactKind {
    /// An instruction (prose instructions collapse the chain to delivered → followed).
    Instruction,
    /// A rule.
    Rule,
    /// A memory.
    Memory,
    /// A tool surface.
    ToolSurface,
    /// A procedure.
    Procedure,
    /// An observation.
    Observation,
}

impl ArtifactKind {
    /// Whether this kind carries an activation signal (§2.6.2). Kinds without one collapse the
    /// chain to delivered → followed.
    pub fn activation_observable(self) -> bool {
        matches!(
            self,
            ArtifactKind::Rule
                | ArtifactKind::Memory
                | ArtifactKind::ToolSurface
                | ArtifactKind::Procedure
        )
    }
}

/// The three chain events, schema-only (§2.6.2). Home planes: delivered/activated on P1,
/// followed on P4. `delivered` carries no detector (it is a P1 emission); `activated` and
/// `followed` carry detector provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainEvent {
    /// `context.artefact.delivered` (P1) — one per artifact per model call; needs a compiled id.
    Delivered {
        /// The artifact's content-addressed semantic id.
        artefact_id: String,
        /// The per-delivery id.
        delivery_id: String,
    },
    /// `context.artefact.activated` (P1) — never emitted when `activation_observable = false`.
    Activated {
        /// The artifact id.
        artefact_id: String,
        /// The delivery id.
        delivery_id: String,
        /// The detector provenance.
        detector: Detector,
    },
    /// `verification.artefact.followed` (P4) — deterministic only via a referenced validator.
    Followed {
        /// The artifact id.
        artefact_id: String,
        /// The delivery id.
        delivery_id: String,
        /// The detector provenance.
        detector: Detector,
    },
}

impl ChainEvent {
    /// The event identifier (`plane.noun.verb`), whose prefix is the home plane.
    pub fn identifier(&self) -> &'static str {
        match self {
            ChainEvent::Delivered { .. } => "context.artefact.delivered",
            ChainEvent::Activated { .. } => "context.artefact.activated",
            ChainEvent::Followed { .. } => "verification.artefact.followed",
        }
    }

    /// The detector, when the event carries one (`delivered` does not).
    pub fn detector(&self) -> Option<Detector> {
        match self {
            ChainEvent::Delivered { .. } => None,
            ChainEvent::Activated { detector, .. } | ChainEvent::Followed { detector, .. } => {
                Some(*detector)
            }
        }
    }
}

/// The typed not-applicable reason (§2.6.3; T-LCD-15). Never 0, never a proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NaReason {
    /// The metric does not apply to the participant's class.
    Class,
    /// The required observability is unavailable.
    Observability,
    /// A required capability is absent.
    Capability,
    /// The required mediation is unavailable.
    Mediation,
    /// The estimator is undefined for this cell.
    EstimatorUndefined,
    /// The stage was not run.
    NotRun,
    /// No detector exists.
    NoDetector,
}

/// A class-scoped metric declaration (§2.6.3; ADR-0045). The Stage-1 slice: every metric
/// carries `applies_to_classes`, `requires_observability` and `detector_classes_allowed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricDeclaration {
    /// The metric name.
    pub name: String,
    /// The classes this metric applies to.
    pub applies_to_classes: BTreeSet<ParticipantClass>,
    /// The observability this metric requires.
    pub requires_observability: BTreeSet<Observability>,
    /// The detector classes allowed.
    pub detector_classes_allowed: BTreeSet<Detector>,
}

impl MetricDeclaration {
    /// Applicability = class ∧ observability (§2.6.3). Returns the typed [`NaReason`] on an
    /// inadmissible cell — never 0, never a proxy (T-LCD-15).
    pub fn applicability(&self, desc: &ParticipantDescriptor) -> Result<(), NaReason> {
        if !self.applies_to_classes.contains(&desc.class) {
            return Err(NaReason::Class);
        }
        if !self
            .requires_observability
            .is_subset(&desc.observability_level)
        {
            return Err(NaReason::Observability);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validity_is_three_valued_and_model_free() {
        // §2.6.1: a version stamp decides valid/invalid; no check ⇒ unknown; no model input.
        let ok = validity(
            &ValidityCheck::VersionStamp {
                stamp: "v3".into(),
                expected: "v3".into(),
            },
            "evt:1",
        );
        assert_eq!(ok.state, Validity::Valid);
        let bad = validity(
            &ValidityCheck::VersionStamp {
                stamp: "v2".into(),
                expected: "v3".into(),
            },
            "evt:1",
        );
        assert_eq!(bad.state, Validity::Invalid);
        let none = validity(&ValidityCheck::None, "evt:1");
        assert_eq!(none.state, Validity::Unknown);
        assert_eq!(none.check_ref, None);
    }

    #[test]
    fn validity_is_identical_across_configurations() {
        // S-077 property: the same artifact version + check yields the same result no matter the
        // configuration/model. Structurally guaranteed (no M/κ param); asserted for two "runs".
        let check = ValidityCheck::ExecutableValidator {
            passed: true,
            check_ref: "chk:schema-1".into(),
        };
        let run_a = validity(&check, "evt:a");
        let run_b = validity(&check, "evt:b");
        assert_eq!(run_a, run_b);
    }

    #[test]
    fn chain_has_three_events_with_detector_provenance() {
        // AC-A2-3 (T-LCD-13): the taxonomy contains the three chain events; activated/followed
        // carry a detector; delivered does not; the home-plane prefixes are P1/P1/P4.
        let d = ChainEvent::Delivered {
            artefact_id: "a".into(),
            delivery_id: "d".into(),
        };
        let a = ChainEvent::Activated {
            artefact_id: "a".into(),
            delivery_id: "d".into(),
            detector: Detector::Deterministic,
        };
        let f = ChainEvent::Followed {
            artefact_id: "a".into(),
            delivery_id: "d".into(),
            detector: Detector::Deterministic,
        };
        assert_eq!(d.identifier(), "context.artefact.delivered");
        assert_eq!(a.identifier(), "context.artefact.activated");
        assert_eq!(f.identifier(), "verification.artefact.followed");
        assert_eq!(d.detector(), None);
        assert_eq!(a.detector(), Some(Detector::Deterministic));
        assert_eq!(f.detector(), Some(Detector::Deterministic));
    }

    #[test]
    fn activation_observable_is_per_kind() {
        assert!(ArtifactKind::ToolSurface.activation_observable());
        assert!(ArtifactKind::Procedure.activation_observable());
        // Prose instructions/observations have no activation signal (collapse the chain).
        assert!(!ArtifactKind::Instruction.activation_observable());
        assert!(!ArtifactKind::Observation.activation_observable());
    }

    #[test]
    fn metric_applicability_returns_typed_na_never_zero() {
        // AC-A2-4 (T-LCD-15): a metric carries applies_to_classes + requires_observability and
        // renders a typed n/a on an inadmissible cell.
        use crate::participant::{describe, HostingMechanism, RawDescriptor};
        let native_only = MetricDeclaration {
            name: "compliance.rate".into(),
            applies_to_classes: [ParticipantClass::Native].into_iter().collect(),
            requires_observability: [Observability::Events].into_iter().collect(),
            detector_classes_allowed: [Detector::Deterministic].into_iter().collect(),
        };
        let hosted = describe(RawDescriptor {
            class: Some(ParticipantClass::Hosted),
            hosting_mechanism: HostingMechanism::SessionAbi,
            observability_level: [Observability::Events].into_iter().collect(),
            capability_vector: Default::default(),
        })
        .unwrap();
        assert_eq!(native_only.applicability(&hosted), Err(NaReason::Class));

        let events_metric = MetricDeclaration {
            requires_observability: [Observability::ModelIo].into_iter().collect(),
            applies_to_classes: [ParticipantClass::Hosted].into_iter().collect(),
            ..native_only.clone()
        };
        assert_eq!(
            events_metric.applicability(&hosted),
            Err(NaReason::Observability)
        );
    }
}
