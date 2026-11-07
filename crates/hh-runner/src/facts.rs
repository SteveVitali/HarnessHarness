//! The §3.3 slot-fact → engine-outcome map (REQ-XB-120; alias REQ-XB-012):
//! every slot-fact row in table order, the premature-refusal precedence
//! (O2 delta RD-17 — a premature budget refusal is `infrastructure_failure`
//! even when a valid reward exists), and the pinned one-to-one Harbor
//! `error_policy` cause map (research/25 F13). No relabelling; only
//! `infrastructure_failure` replans.

use crate::disposition::{
    AdmissionReason, Disposition, EndCause, InconclusiveCause, InfraCause, SlotFlag, WithheldReason,
};
use hharness_ontology::control::{
    CancelledBy, InfraError, InfraErrorFamily, OutcomeClass, StopReason,
};

/// `resolved | unresolved` — the `executed{…}` member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// `resolved` — the task's success criterion held.
    Resolved,
    /// `unresolved` — a valid zero/negative verdict.
    Unresolved,
}

/// The observed slot facts the map consumes. A slot presents exactly the
/// facts its execution produced; the map evaluates them in the §3.3 order
/// with the premature-refusal precedence.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SlotFacts {
    /// A valid reward exists (any end cause, including timeout or a
    /// non-premature budget refusal). Carries its resolution + end cause.
    pub reward: Option<RewardFact>,
    /// The slot budget refusal was premature: the slot still had ≥ 1
    /// worst-case-call reservation of headroom under its slice (REQ-XC-126).
    /// Takes precedence over `reward` (RD-17).
    pub premature_budget_refusal: bool,
    /// The verifier stayed unavailable after the regrade bound (`<c>`).
    pub verifier_unavailable: Option<String>,
    /// Provision failure, cell death, runner crash, or relay death not
    /// preceded by an agent-issued signal.
    pub infra: Option<InfraCause>,
    /// The relay died after an agent-issued signal to PID 1 (REQ-XB-129).
    pub relay_killed_by_agent: bool,
    /// An ST0 admission refusal.
    pub admission_refusal: Option<AdmissionReason>,
    /// Operator pause or cancel.
    pub cancelled: Option<CancelledBy>,
}

/// The reward fact — `reward_ppm` carries the exact value; resolution is
/// the recorded classification (`reward_ppm > 0` in the fixture channels).
#[derive(Debug, Clone, PartialEq)]
pub struct RewardFact {
    /// The resolved classification.
    pub resolution: Resolution,
    /// The agent-phase end cause.
    pub end_cause: EndCause,
    /// The exact reward (ppm of 1.0), when the channel produced one.
    pub reward_ppm: Option<u64>,
    /// Extra flags the channel/integrity legs raised (verbatim spellings).
    pub flags: Vec<SlotFlag>,
}

/// The map result — what the runner writes and the engine derives.
#[derive(Debug, Clone, PartialEq)]
pub struct OutcomeMap {
    /// The engine outcome class (the subject run's recorded class).
    pub outcome_class: OutcomeClass,
    /// The `stop_reason` the subject run's `lifecycle.run.finished` carries
    /// (the engine re-derives the class from it — one derivation, CC1).
    pub stop_reason: StopReason,
    /// Whether the finished row's `oracle_failed` member is set.
    pub oracle_failed: bool,
    /// The slot disposition spelling.
    pub disposition: Disposition,
    /// `true` only for `infrastructure_failure` — the single class that
    /// replans (REQ-XB-120).
    pub replans: bool,
}

fn infra_error(cause: &InfraCause) -> InfraError {
    let family = match cause {
        InfraCause::RunnerCrash | InfraCause::RelayDied => InfraErrorFamily::Kernel,
        InfraCause::Provision(_) | InfraCause::EnvironmentDied => InfraErrorFamily::Env,
        InfraCause::BudgetReservation | InfraCause::ModelRoute(_) | InfraCause::UnreceiptedCall => {
            InfraErrorFamily::Model
        }
        InfraCause::Named(_) => InfraErrorFamily::Participant,
    };
    InfraError {
        family,
        class: cause.as_str(),
    }
}

fn map(
    outcome_class: OutcomeClass,
    stop_reason: StopReason,
    disposition: Disposition,
) -> OutcomeMap {
    OutcomeMap {
        outcome_class,
        stop_reason,
        oracle_failed: outcome_class == OutcomeClass::OracleFailure,
        disposition,
        replans: outcome_class == OutcomeClass::InfrastructureFailure,
    }
}

/// `map_slot_facts(facts) → OutcomeMap` — the §3.3 table, rows evaluated in
/// table order except that the premature-refusal row precedes row 1.
///
/// Table (fact → class → disposition):
/// 1. valid reward (any end cause) → `scored` → `executed{resolved|unresolved}`;
/// 2. verifier unavailable after regrades → `oracle_failure` →
///    `inconclusive{verifier_unavailable:<c>}`;
/// 3. provision failure / cell death / runner crash / relay death →
///    `infrastructure_failure` → `inconclusive{infra:<c>}` (replanned);
/// 4. relay death after an agent-issued signal → `scored` → `executed{…}`
///    with `end_cause: crashed`, flagged `relay_killed_by_agent`;
/// 5. premature slot budget refusal → `infrastructure_failure` →
///    `inconclusive{infra:budget_reservation}` — precedence over row 1;
/// 6. ST0 admission refusal → `refused` → `withheld{admission:<reason>}`;
/// 7. operator pause/cancel → `cancelled` → `inconclusive{cancelled}`.
pub fn map_slot_facts(facts: &SlotFacts) -> OutcomeMap {
    // Row 5 first (RD-17): a premature refusal is infrastructure even when
    // a valid reward exists — the reward is recorded in the slot record's
    // verdict section but never decides the outcome.
    if facts.premature_budget_refusal {
        let cause = InfraCause::BudgetReservation;
        return map(
            OutcomeClass::InfrastructureFailure,
            StopReason::InfrastructureFailure {
                error_class: infra_error(&cause),
            },
            Disposition::Inconclusive {
                cause: InconclusiveCause::Infra(cause),
            },
        );
    }
    // Row 1: a valid reward scores under every end cause.
    if let Some(r) = &facts.reward {
        let crashed_by_agent = facts.relay_killed_by_agent;
        let end_cause = if crashed_by_agent {
            EndCause::Crashed
        } else {
            r.end_cause
        };
        return map(
            OutcomeClass::Scored,
            StopReason::Completed,
            Disposition::Executed {
                resolved: r.resolution == Resolution::Resolved,
                end_cause,
            },
        );
    }
    // Row 2: verifier unavailable after regrades.
    if let Some(c) = &facts.verifier_unavailable {
        return map(
            OutcomeClass::OracleFailure,
            StopReason::Completed,
            Disposition::Inconclusive {
                cause: InconclusiveCause::VerifierUnavailable(c.clone()),
            },
        );
    }
    // Row 3: infrastructure endings (relay death without an agent signal
    // lands here; the flagged variant scores under row 4 below).
    if let Some(cause) = &facts.infra {
        return map(
            OutcomeClass::InfrastructureFailure,
            StopReason::InfrastructureFailure {
                error_class: infra_error(cause),
            },
            Disposition::Inconclusive {
                cause: InconclusiveCause::Infra(cause.clone()),
            },
        );
    }
    // Row 4: relay death after an agent-issued signal — scored `crashed`
    // (reached only when no valid reward exists; with a reward row 1
    // already fired with `end_cause: crashed`).
    if facts.relay_killed_by_agent {
        return map(
            OutcomeClass::Scored,
            StopReason::Completed,
            Disposition::Executed {
                resolved: false,
                end_cause: EndCause::Crashed,
            },
        );
    }
    // Row 6: ST0 admission refusal — no cell, nothing fabricated.
    if let Some(reason) = facts.admission_refusal {
        return map(
            OutcomeClass::Refused,
            StopReason::Refused {
                blocking_effect_id: "slot_admission".to_string(),
            },
            Disposition::Withheld {
                reason: WithheldReason::Admission(reason),
            },
        );
    }
    // Row 7: operator pause or cancel.
    if let Some(by) = facts.cancelled {
        return map(
            OutcomeClass::Cancelled,
            StopReason::Cancelled { by },
            Disposition::Inconclusive {
                cause: InconclusiveCause::Cancelled,
            },
        );
    }
    // No fact presented — an unfinished attempt is never mapped (the
    // caller closes it `infra:runner_crash` or waits for the seal).
    map(
        OutcomeClass::InfrastructureFailure,
        StopReason::InfrastructureFailure {
            error_class: infra_error(&InfraCause::RunnerCrash),
        },
        Disposition::Inconclusive {
            cause: InconclusiveCause::Infra(InfraCause::RunnerCrash),
        },
    )
}

/// The pinned Harbor `error_policy` causes (research/25 F13; REQ-XB-120 —
/// "Harbor failure causes map one-to-one through the pinned cause map").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HarborErrorCause {
    /// `agent_exhausted_budget` — a valid reward (no retry): scored with
    /// `end_cause: budget_refused`.
    AgentExhaustedBudget,
    /// `sandbox_failed` — invalid, retry: `infrastructure_failure`.
    SandboxFailed,
    /// `model_provider_failed` — invalid, retry per type:
    /// `infrastructure_failure`.
    ModelProviderFailed,
    /// `verifier_failed` — invalid, grade: `oracle_failure`.
    VerifierFailed,
    /// `unclassified` (`NonZeroAgentExitCodeError`) — invalid, retry:
    /// `infrastructure_failure`.
    Unclassified,
}

impl HarborErrorCause {
    /// The canonical Harbor spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            HarborErrorCause::AgentExhaustedBudget => "agent_exhausted_budget",
            HarborErrorCause::SandboxFailed => "sandbox_failed",
            HarborErrorCause::ModelProviderFailed => "model_provider_failed",
            HarborErrorCause::VerifierFailed => "verifier_failed",
            HarborErrorCause::Unclassified => "unclassified",
        }
    }

    /// Every member — the pinned set.
    pub const ALL: [HarborErrorCause; 5] = [
        HarborErrorCause::AgentExhaustedBudget,
        HarborErrorCause::SandboxFailed,
        HarborErrorCause::ModelProviderFailed,
        HarborErrorCause::VerifierFailed,
        HarborErrorCause::Unclassified,
    ];

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<HarborErrorCause> {
        Self::ALL.iter().find(|c| c.as_str() == s).copied()
    }
}

/// `harbor_cause_map(cause) → OutcomeMap` — the one-to-one pinned map from
/// Harbor `error_policy` causes onto HH outcome classes and dispositions.
/// `resolved` is the reward's recorded resolution for
/// `agent_exhausted_budget` (a scored refusal resolves by its reward).
pub fn harbor_cause_map(cause: HarborErrorCause, resolved: bool) -> OutcomeMap {
    match cause {
        HarborErrorCause::AgentExhaustedBudget => map(
            OutcomeClass::Scored,
            StopReason::Completed,
            Disposition::Executed {
                resolved,
                end_cause: EndCause::BudgetRefused,
            },
        ),
        HarborErrorCause::VerifierFailed => map(
            OutcomeClass::OracleFailure,
            StopReason::Completed,
            Disposition::Inconclusive {
                cause: InconclusiveCause::VerifierUnavailable(cause.as_str().to_string()),
            },
        ),
        HarborErrorCause::SandboxFailed
        | HarborErrorCause::ModelProviderFailed
        | HarborErrorCause::Unclassified => {
            let infra = InfraCause::Named(cause.as_str().to_string());
            map(
                OutcomeClass::InfrastructureFailure,
                StopReason::InfrastructureFailure {
                    error_class: infra_error(&infra),
                },
                Disposition::Inconclusive {
                    cause: InconclusiveCause::Infra(infra),
                },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reward(resolved: bool, end: EndCause) -> Option<RewardFact> {
        Some(RewardFact {
            resolution: if resolved {
                Resolution::Resolved
            } else {
                Resolution::Unresolved
            },
            end_cause: end,
            reward_ppm: Some(if resolved { 1_000_000 } else { 0 }),
            flags: vec![],
        })
    }

    #[test]
    fn every_table_row_maps_once() {
        // Row 1 — valid reward under each end cause.
        for (end, resolved, spelling) in [
            (EndCause::Exited(0), true, "executed{resolved}"),
            (EndCause::AgentTimeout, true, "executed{resolved}"),
            (EndCause::BudgetRefused, false, "executed{unresolved}"),
            (EndCause::Crashed, false, "executed{unresolved}"),
        ] {
            let m = map_slot_facts(&SlotFacts {
                reward: reward(resolved, end),
                ..Default::default()
            });
            assert_eq!(m.outcome_class, OutcomeClass::Scored);
            assert_eq!(m.disposition.as_str(), spelling);
            assert!(!m.replans);
        }
        // Row 2 — verifier unavailable after regrades.
        let m = map_slot_facts(&SlotFacts {
            verifier_unavailable: Some("reward_missing".into()),
            ..Default::default()
        });
        assert_eq!(m.outcome_class, OutcomeClass::OracleFailure);
        assert_eq!(
            m.disposition.as_str(),
            "inconclusive{verifier_unavailable:reward_missing}"
        );
        assert!(!m.replans);
        // Row 3 — infrastructure endings.
        for (cause, spelling) in [
            (
                InfraCause::Provision("podman_pull".into()),
                "inconclusive{infra:provision:podman_pull}",
            ),
            (
                InfraCause::EnvironmentDied,
                "inconclusive{infra:environment_died}",
            ),
            (InfraCause::RunnerCrash, "inconclusive{infra:runner_crash}"),
            (InfraCause::RelayDied, "inconclusive{infra:relay_died}"),
        ] {
            let m = map_slot_facts(&SlotFacts {
                infra: Some(cause),
                ..Default::default()
            });
            assert_eq!(m.outcome_class, OutcomeClass::InfrastructureFailure);
            assert_eq!(m.disposition.as_str(), spelling);
            assert!(m.replans, "{spelling}");
        }
        // Row 4 — relay death after an agent-issued signal: scored crashed.
        let m = map_slot_facts(&SlotFacts {
            relay_killed_by_agent: true,
            ..Default::default()
        });
        assert_eq!(m.outcome_class, OutcomeClass::Scored);
        assert_eq!(m.disposition.as_str(), "executed{unresolved}");
        assert_eq!(m.disposition.end_cause(), Some(EndCause::Crashed));
        assert!(!m.replans);
        // With a reward present the flag still lands `crashed` scored.
        let m = map_slot_facts(&SlotFacts {
            reward: reward(true, EndCause::Exited(0)),
            relay_killed_by_agent: true,
            ..Default::default()
        });
        assert_eq!(m.outcome_class, OutcomeClass::Scored);
        assert_eq!(m.disposition.end_cause(), Some(EndCause::Crashed));
        // Row 5 — premature refusal beats a valid reward (RD-17).
        let m = map_slot_facts(&SlotFacts {
            reward: reward(true, EndCause::Exited(0)),
            premature_budget_refusal: true,
            ..Default::default()
        });
        assert_eq!(m.outcome_class, OutcomeClass::InfrastructureFailure);
        assert_eq!(
            m.disposition.as_str(),
            "inconclusive{infra:budget_reservation}"
        );
        assert!(m.replans);
        // Row 6 — ST0 refusal.
        let m = map_slot_facts(&SlotFacts {
            admission_refusal: Some(AdmissionReason::TaskDigestMismatch),
            ..Default::default()
        });
        assert_eq!(m.outcome_class, OutcomeClass::Refused);
        assert_eq!(
            m.disposition.as_str(),
            "withheld{admission:task_digest_mismatch}"
        );
        assert!(!m.replans);
        // Row 7 — operator cancel.
        let m = map_slot_facts(&SlotFacts {
            cancelled: Some(CancelledBy::Operator),
            ..Default::default()
        });
        assert_eq!(m.outcome_class, OutcomeClass::Cancelled);
        assert_eq!(m.disposition.as_str(), "inconclusive{cancelled}");
        assert!(!m.replans);
    }

    #[test]
    fn harbor_causes_map_one_to_one() {
        let expected = [
            (
                HarborErrorCause::AgentExhaustedBudget,
                OutcomeClass::Scored,
                "executed{resolved}",
                false,
            ),
            (
                HarborErrorCause::SandboxFailed,
                OutcomeClass::InfrastructureFailure,
                "inconclusive{infra:sandbox_failed}",
                true,
            ),
            (
                HarborErrorCause::ModelProviderFailed,
                OutcomeClass::InfrastructureFailure,
                "inconclusive{infra:model_provider_failed}",
                true,
            ),
            (
                HarborErrorCause::VerifierFailed,
                OutcomeClass::OracleFailure,
                "inconclusive{verifier_unavailable:verifier_failed}",
                false,
            ),
            (
                HarborErrorCause::Unclassified,
                OutcomeClass::InfrastructureFailure,
                "inconclusive{infra:unclassified}",
                true,
            ),
        ];
        for (cause, class, disposition, replans) in expected {
            let m = harbor_cause_map(cause, true);
            assert_eq!(m.outcome_class, class, "{cause:?}");
            assert_eq!(m.disposition.as_str(), disposition, "{cause:?}");
            assert_eq!(m.replans, replans, "{cause:?}");
        }
        // One-to-one: distinct causes never share a mapped class+disposition.
        let mut seen = std::collections::BTreeSet::new();
        for c in HarborErrorCause::ALL {
            let m = harbor_cause_map(c, false);
            assert!(seen.insert((m.outcome_class, m.disposition.as_str())));
        }
    }
}
