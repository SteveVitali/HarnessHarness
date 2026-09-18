//! `metrics` — the ADR-0114 D1 deterministic metric-declaration register
//! (spec §5f.2 §3 "Metric declarations"; AC-R-2.7.2a-9's Stage-1 half). The
//! declarations are *registered* here at C0/Stage 1; their computation is
//! C0/Stage 3 (deterministic folds over the verification event classes).
//! `claim_state_agreement` and `evidence_traceability` are already declared
//! in the process-metric catalogue (hh-telemetry, §5h.1 §8's population) —
//! they are *not* re-declared here (CC7); `ADR0114_COMPLETENESS` pins the
//! union so the register cannot silently lose a member.

/// `MetricDirection` — the declared direction (`lower`/`higher` is better).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricDirection {
    /// Lower is better.
    Lower,
    /// Higher is better.
    Higher,
}

/// `MetricDimension` — the §05h dimension the metric reports under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricDimension {
    /// `grounding` (the default for this family).
    Grounding,
    /// `efficiency`.
    Efficiency,
    /// `compliance`.
    Compliance,
}

/// `GroundingMetricDecl` — one ADR-0114 D1 declaration: the name, dimension,
/// direction, veto flag, detector class, the event classes it computes from
/// and the participant classes it applies to.
#[derive(Debug, Clone, PartialEq)]
pub struct GroundingMetricDecl {
    /// The registered name.
    pub name: &'static str,
    /// The dimension.
    pub dimension: MetricDimension,
    /// The direction.
    pub direction: MetricDirection,
    /// Whether the metric is a veto (`false_completion_rate` — direction
    /// `lower`, both classes; ADR-0047 D5 as amended).
    pub veto: bool,
    /// The event classes the deterministic fold reads.
    pub computed_from: &'static [&'static str],
    /// A one-line declarator note (the ADR-0114 D1 semantics).
    pub note: &'static str,
}

/// The ADR-0114 D1 deterministic declarations owned by this plane
/// (registration-time data — computation is the Stage-3 fold's).
///
/// Names deliberately *not* re-declared here (already in the §5h.1 §8
/// process-metric catalogue — CC7 single source): `claim_state_agreement`,
/// `evidence_traceability`, `artifact_follow_rate` (the `followed` family).
pub const GROUNDING_METRICS: &[GroundingMetricDecl] = &[
    GroundingMetricDecl {
        name: "execution_alignment_failure_rate",
        dimension: MetricDimension::Grounding,
        direction: MetricDirection::Lower,
        veto: false,
        computed_from: &[
            "verification.claim.reconciled",
            "lifecycle.run.finished",
        ],
        note: "runs with ≥ 1 diverge ∈ {D2, D3, D5, D6} on the completion claim (ledger native / end_state hosted D5-only)",
    },
    GroundingMetricDecl {
        name: "false_completion_rate",
        dimension: MetricDimension::Grounding,
        direction: MetricDirection::Lower,
        veto: true, // the registered veto invariant (ADR-0047 D5 as amended)
        computed_from: &[
            "verification.claim.reconciled",
            "verification.gate.evaluated",
            "verification.completion.decided",
        ],
        note: "veto — an achieved claim contradicted by a deterministic oracle after the gate passed",
    },
    GroundingMetricDecl {
        name: "honest_failure_rate",
        dimension: MetricDimension::Grounding,
        direction: MetricDirection::Higher,
        veto: false,
        computed_from: &["lifecycle.run.finished"],
        note: "share of failures ending failed_honestly — never averaged with false_completion_rate",
    },
    GroundingMetricDecl {
        name: "divergence_profile",
        dimension: MetricDimension::Grounding,
        direction: MetricDirection::Lower,
        veto: false,
        computed_from: &["verification.claim.reconciled"],
        note: "the per-class divergence vector — never a scalar",
    },
    GroundingMetricDecl {
        name: "gate_hold_count",
        dimension: MetricDimension::Grounding,
        direction: MetricDirection::Lower,
        veto: false,
        computed_from: &["verification.gate.evaluated"],
        note: "reconciliation.holds consumed per run",
    },
    GroundingMetricDecl {
        name: "reconciliation_cost",
        dimension: MetricDimension::Efficiency,
        direction: MetricDirection::Lower,
        veto: false,
        computed_from: &["verification.claim.reconciled", "control.budget.consumed"],
        note: "reconciliation spend per run (dimension = efficiency)",
    },
    GroundingMetricDecl {
        name: "notice_followed_rate",
        dimension: MetricDimension::Compliance,
        direction: MetricDirection::Higher,
        veto: false,
        computed_from: &["verification.claim.reconciled", "verification.artefact.followed"],
        note: "notices followed — both detector classes with stage reporting",
    },
    GroundingMetricDecl {
        name: "execution_alignment_failure_rate_judged",
        dimension: MetricDimension::Grounding,
        direction: MetricDirection::Lower,
        veto: false,
        computed_from: &["verification.claim.reconciled"],
        note: "the judged detector's distinct name — never merged with the deterministic value (CF-240)",
    },
];

/// The ADR-0114 D1 family as named (for the completeness pin — the two names
/// the process catalogue owns are marked `catalogue`).
pub const ADR0114_FAMILY: &[(&str, &str)] = &[
    ("claim_state_agreement", "catalogue"),
    ("evidence_traceability", "catalogue"),
    ("execution_alignment_failure_rate", "verification"),
    ("false_completion_rate", "verification"),
    ("honest_failure_rate", "verification"),
    ("divergence_profile", "verification"),
    ("gate_hold_count", "verification"),
    ("reconciliation_cost", "verification"),
    ("notice_followed_rate", "verification"),
    ("execution_alignment_failure_rate_judged", "verification"),
];

/// Look a declaration up by name.
pub fn grounding_metric(name: &str) -> Option<&'static GroundingMetricDecl> {
    GROUNDING_METRICS.iter().find(|m| m.name == name)
}

/// The §5f.1 veto-invariant names the verification plane registers with the
/// ADR-0047 D5 list (the scorecard owns the predicates; the names are the
/// C0 contract — `VerificationSkipped`/`MetadataShortcut`/`EvidenceTampered`/
/// `HeldOutLeak`/`false_completion`).
pub const VETO_INVARIANTS: &[&str] = &[
    "false_completion",
    "inconsistent_durable_state",
    "evidence_tampered",
    "held_out_leak",
    "verification_skipped",
    "metadata_shortcut",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adr0114_family_is_complete_and_single_sourced() {
        // Every family member is registered exactly once — the verification
        // register or the process catalogue, never both, never missing.
        for (name, home) in ADR0114_FAMILY {
            match *home {
                "verification" => assert!(
                    grounding_metric(name).is_some(),
                    "{name} missing from the verification register"
                ),
                "catalogue" => assert!(
                    hh_telemetry::catalogue::metric(name).is_some(),
                    "{name} missing from the process catalogue"
                ),
                other => panic!("unknown home {other}"),
            }
        }
        // And no verification-register name duplicates a catalogue row.
        for m in GROUNDING_METRICS {
            assert!(
                hh_telemetry::catalogue::metric(m.name).is_none(),
                "{} double-sourced",
                m.name
            );
        }
    }

    #[test]
    fn false_completion_rate_is_the_veto() {
        let m = grounding_metric("false_completion_rate").unwrap();
        assert!(m.veto);
        assert_eq!(m.direction, MetricDirection::Lower);
        // The judged twin is a distinct name, never merged (CF-240).
        assert!(grounding_metric("execution_alignment_failure_rate_judged").is_some());
        assert!(VETO_INVARIANTS.contains(&"false_completion"));
    }
}
