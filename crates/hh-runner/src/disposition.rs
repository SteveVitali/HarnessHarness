//! The `hh-experiment-slot/1` typed-disposition vocabulary — the closed
//! spelling sets of the experiments supplement §3.3 slot-fact table and
//! Appendix D's "Dispositions" reserved-strings row. This crate owns the
//! closed `withheld{…}` member set (manifest 261 Notes; consumers fold it,
//! never mint a member).
//!
//! Every type has a canonical spelling (`as_str`/`to_json`) and a strict
//! parse (`parse`/`from_json`): an unknown member is a typed refusal, never
//! a silent relabel (REQ-XB-120 — "no relabelling").

use hharness_wire::json::Json;

/// A refused member value — carries the rejected spelling verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct VocabularyError {
    /// The vocabulary the value was refused under.
    pub vocabulary: &'static str,
    /// The rejected value.
    pub value: String,
}

impl std::fmt::Display for VocabularyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} is not a member", self.vocabulary, self.value)
    }
}

impl std::error::Error for VocabularyError {}

fn refuse(vocabulary: &'static str, value: &str) -> VocabularyError {
    VocabularyError {
        vocabulary,
        value: value.to_string(),
    }
}

// ── Agent-phase end causes (§3.3 ST3) ───────────────────────────────────────

/// `end_cause ∈ {exited(rc), agent_timeout, budget_refused, crashed}` — the
/// closed ST3 set; `relay_killed_by_agent`-flagged slots close `crashed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EndCause {
    /// `exited{rc}` — the participant exited on its own.
    Exited(i64),
    /// `agent_timeout` — the agent deadline lapsed (REQ-XB-119).
    AgentTimeout,
    /// `budget_refused` — a non-premature slot budget refusal ended the
    /// agent phase; the slot still verifies and scores.
    BudgetRefused,
    /// `crashed` — the participant (or the relay, killed by the agent)
    /// crashed.
    Crashed,
}

impl EndCause {
    /// The canonical spelling.
    pub fn as_str(self) -> String {
        match self {
            EndCause::Exited(rc) => format!("exited{{{rc}}}"),
            EndCause::AgentTimeout => "agent_timeout".to_string(),
            EndCause::BudgetRefused => "budget_refused".to_string(),
            EndCause::Crashed => "crashed".to_string(),
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<EndCause> {
        match s {
            "agent_timeout" => Some(EndCause::AgentTimeout),
            "budget_refused" => Some(EndCause::BudgetRefused),
            "crashed" => Some(EndCause::Crashed),
            _ => s
                .strip_prefix("exited{")?
                .strip_suffix('}')
                .and_then(|r| r.parse::<i64>().ok())
                .map(EndCause::Exited),
        }
    }
}

// ── ST0 admission refusal reasons (§3.3 ST0 + the contract's closed set) ─────

/// The `admission:<r>` reason set — `AG<n>` (n ∈ 1..=9, the admission gates,
/// REQ-XH-127(a)/RD-11), the named holds, and the §3.3 ST0 check refusals.
/// This is the whole closed set; a consumer never mints a member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AdmissionReason {
    /// `AG<n>` — an admission gate refused (n ∈ 1..=9).
    Gate(u8),
    /// `harness_layer_mismatch` — ST2 verify-only layer/bundle/version check
    /// failed (§5.2, REQ-XC-305).
    HarnessLayerMismatch,
    /// `native_lane_hold` — the HH-native lane is held (REQ-XH-124).
    NativeLaneHold,
    /// `pp0_infra_hold` — the PP0 infra hold (REQ-XH-127(a)).
    Pp0InfraHold,
    /// `task_digest_mismatch` — ST0: task bytes ≠ the pinned digest.
    TaskDigestMismatch,
    /// `image_not_local` — ST0: the task image digest is not resolvable
    /// locally (pull-by-digest admits no tag).
    ImageNotLocal,
    /// `deadline_admission` — ST0: the declared deadline failed admission.
    DeadlineAdmission,
    /// `egress_profile` — ST0: the egress profile composition or its
    /// `validate()` refused.
    EgressProfile,
    /// `slot_slice` — ST0: the proxy slot-slice reservation refused.
    SlotSlice,
}

impl AdmissionReason {
    /// The canonical spelling after `admission:`.
    pub fn as_str(self) -> String {
        match self {
            AdmissionReason::Gate(n) => format!("AG{n}"),
            AdmissionReason::HarnessLayerMismatch => "harness_layer_mismatch".to_string(),
            AdmissionReason::NativeLaneHold => "native_lane_hold".to_string(),
            AdmissionReason::Pp0InfraHold => "pp0_infra_hold".to_string(),
            AdmissionReason::TaskDigestMismatch => "task_digest_mismatch".to_string(),
            AdmissionReason::ImageNotLocal => "image_not_local".to_string(),
            AdmissionReason::DeadlineAdmission => "deadline_admission".to_string(),
            AdmissionReason::EgressProfile => "egress_profile".to_string(),
            AdmissionReason::SlotSlice => "slot_slice".to_string(),
        }
    }

    /// Every member — the closed set, for coverage tests.
    pub const ALL: [AdmissionReason; 12] = [
        AdmissionReason::Gate(1),
        AdmissionReason::HarnessLayerMismatch,
        AdmissionReason::NativeLaneHold,
        AdmissionReason::Pp0InfraHold,
        AdmissionReason::TaskDigestMismatch,
        AdmissionReason::ImageNotLocal,
        AdmissionReason::DeadlineAdmission,
        AdmissionReason::EgressProfile,
        AdmissionReason::SlotSlice,
        AdmissionReason::Gate(2),
        AdmissionReason::Gate(3),
        AdmissionReason::Gate(4),
    ];

    /// Parse; `Err` on any other input (never a silent admission).
    pub fn parse(s: &str) -> Result<AdmissionReason, VocabularyError> {
        let found = match s {
            "harness_layer_mismatch" => Some(AdmissionReason::HarnessLayerMismatch),
            "native_lane_hold" => Some(AdmissionReason::NativeLaneHold),
            "pp0_infra_hold" => Some(AdmissionReason::Pp0InfraHold),
            "task_digest_mismatch" => Some(AdmissionReason::TaskDigestMismatch),
            "image_not_local" => Some(AdmissionReason::ImageNotLocal),
            "deadline_admission" => Some(AdmissionReason::DeadlineAdmission),
            "egress_profile" => Some(AdmissionReason::EgressProfile),
            "slot_slice" => Some(AdmissionReason::SlotSlice),
            _ => s.strip_prefix("AG").and_then(|n| {
                n.parse::<u8>()
                    .ok()
                    .and_then(|n| (1..=9).contains(&n).then_some(AdmissionReason::Gate(n)))
            }),
        };
        found.ok_or_else(|| refuse("admission_reason", s))
    }
}

// ── The closed `withheld{…}` member set (contract Notes; Appendix D) ─────────

/// `withheld{<reason>}` — the complete closed member set of the supplement
/// (Appendix D "Dispositions" row; §3.3; §10.1.4 REQ rows). Later rows cite
/// this set and never mint a member; gate dispositions and outcomes
/// (`fallback`, `end_after_w2`, `declined`) are NOT members.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WithheldReason {
    /// `admission:<r>` — an ST0 (or named-hold) admission refusal.
    Admission(AdmissionReason),
    /// `budget` — truncated by the spend cap (REQ-XD-040, REQ-XH-117).
    Budget,
    /// `window_deadline` — not admitted by the window cut-off (REQ-XB-128).
    WindowDeadline,
    /// `image_drift` — task image bytes drifted from the pin (REQ-XB-124).
    ImageDrift,
    /// `dataset_private` — the dataset stays private (REQ-XF-132).
    DatasetPrivate,
    /// `no_candidate` — no level had a positive exploration estimate.
    NoCandidate,
    /// `inert_predicted` — below the activation floor (REQ-XD-209).
    InertPredicted,
    /// `inert_at_admissible_window` — no admissible W (§8.3 W rule).
    InertAtAdmissibleWindow,
    /// `infra` — withheld on infrastructure grounds (typed disposition).
    Infra,
    /// `l1_not_passed` — the L1 fallback held the level out (REQ-XH-123).
    L1NotPassed,
    /// `sealed_until_results` — outcome fields sealed until FP-RESULTS
    /// (REQ-XH-121).
    SealedUntilResults,
}

impl WithheldReason {
    /// The canonical spelling inside `withheld{…}`.
    pub fn as_str(self) -> String {
        match self {
            WithheldReason::Admission(r) => format!("admission:{}", r.as_str()),
            WithheldReason::Budget => "budget".to_string(),
            WithheldReason::WindowDeadline => "window_deadline".to_string(),
            WithheldReason::ImageDrift => "image_drift".to_string(),
            WithheldReason::DatasetPrivate => "dataset_private".to_string(),
            WithheldReason::NoCandidate => "no_candidate".to_string(),
            WithheldReason::InertPredicted => "inert_predicted".to_string(),
            WithheldReason::InertAtAdmissibleWindow => "inert_at_admissible_window".to_string(),
            WithheldReason::Infra => "infra".to_string(),
            WithheldReason::L1NotPassed => "l1_not_passed".to_string(),
            WithheldReason::SealedUntilResults => "sealed_until_results".to_string(),
        }
    }

    /// The non-`admission` members — the closed tail of the set (the
    /// `admission:<r>` family enumerates through [`AdmissionReason`]).
    pub const NON_ADMISSION: [WithheldReason; 10] = [
        WithheldReason::Budget,
        WithheldReason::WindowDeadline,
        WithheldReason::ImageDrift,
        WithheldReason::DatasetPrivate,
        WithheldReason::NoCandidate,
        WithheldReason::InertPredicted,
        WithheldReason::InertAtAdmissibleWindow,
        WithheldReason::Infra,
        WithheldReason::L1NotPassed,
        WithheldReason::SealedUntilResults,
    ];

    /// Parse the inside of `withheld{…}`; `Err` outside the closed set.
    pub fn parse(s: &str) -> Result<WithheldReason, VocabularyError> {
        if let Some(r) = s.strip_prefix("admission:") {
            return AdmissionReason::parse(r).map(WithheldReason::Admission);
        }
        let found = Self::NON_ADMISSION
            .iter()
            .find(|r| r.as_str() == s)
            .copied();
        found.ok_or_else(|| refuse("withheld_reason", s))
    }
}

// ── `infra:<c>` causes (§3.3 row 3/5 + Appendix D) ───────────────────────────

/// The `inconclusive{infra:<c>}` cause vocabulary — the named members are
/// the §3.3 spellings; [`InfraCause::Named`] carries a cause declared by
/// another row (e.g. the cell crate's provision causes, R6.9) verbatim —
/// the map in this crate only produces the pinned members.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum InfraCause {
    /// `provision:<c>` — ST1 provision failed; `<c>` is the cell-side cause.
    Provision(String),
    /// `environment_died` — the cell died mid-agent-phase (§3.3 ST3).
    EnvironmentDied,
    /// `runner_crash` — the runner/worker process died (REQ-XB-105).
    RunnerCrash,
    /// `relay_died` — relay death not preceded by an agent-issued signal.
    RelayDied,
    /// `budget_reservation` — premature slot budget refusal (REQ-XC-126).
    BudgetReservation,
    /// `model_route:<d>` — the model route failed (Appendix D).
    ModelRoute(String),
    /// `unreceipted_call` — a call with no receipt (Appendix D).
    UnreceiptedCall,
    /// `<c>` — a cause declared and owned by a later row, carried verbatim
    /// (non-empty, no `{`/`}` characters).
    Named(String),
}

impl InfraCause {
    /// The canonical spelling after `infra:`.
    pub fn as_str(&self) -> String {
        match self {
            InfraCause::Provision(c) => format!("provision:{c}"),
            InfraCause::EnvironmentDied => "environment_died".to_string(),
            InfraCause::RunnerCrash => "runner_crash".to_string(),
            InfraCause::RelayDied => "relay_died".to_string(),
            InfraCause::BudgetReservation => "budget_reservation".to_string(),
            InfraCause::ModelRoute(d) => format!("model_route:{d}"),
            InfraCause::UnreceiptedCall => "unreceipted_call".to_string(),
            InfraCause::Named(c) => c.clone(),
        }
    }

    /// Parse after `infra:`; `Err` on empty or brace-carrying input.
    pub fn parse(s: &str) -> Result<InfraCause, VocabularyError> {
        if s.is_empty() || s.contains('{') || s.contains('}') {
            return Err(refuse("infra_cause", s));
        }
        if let Some(c) = s.strip_prefix("provision:") {
            if c.is_empty() || c.contains('{') || c.contains('}') {
                return Err(refuse("infra_cause", s));
            }
            return Ok(InfraCause::Provision(c.to_string()));
        }
        if let Some(d) = s.strip_prefix("model_route:") {
            if d.is_empty() || d.contains('{') || d.contains('}') {
                return Err(refuse("infra_cause", s));
            }
            return Ok(InfraCause::ModelRoute(d.to_string()));
        }
        Ok(match s {
            "environment_died" => InfraCause::EnvironmentDied,
            "runner_crash" => InfraCause::RunnerCrash,
            "relay_died" => InfraCause::RelayDied,
            "budget_reservation" => InfraCause::BudgetReservation,
            "unreceipted_call" => InfraCause::UnreceiptedCall,
            other => InfraCause::Named(other.to_string()),
        })
    }
}

// ── `inconclusive{<cause>}` (Appendix D) ────────────────────────────────────

/// The `inconclusive{<cause>}` vocabulary: `infra:<c>`,
/// `verifier_unavailable:<c>`, `backend_terminated`, `teardown`,
/// `cancelled`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum InconclusiveCause {
    /// `infra:<c>` — an infrastructure cause (see [`InfraCause`]).
    Infra(InfraCause),
    /// `verifier_unavailable:<c>` — the verifier stayed unavailable after
    /// the regrade bound.
    VerifierUnavailable(String),
    /// `backend_terminated` — the backend terminated the cell.
    BackendTerminated,
    /// `teardown` — ended in teardown.
    Teardown,
    /// `cancelled` — operator pause or cancel.
    Cancelled,
}

impl InconclusiveCause {
    /// The canonical spelling inside `inconclusive{…}`.
    pub fn as_str(&self) -> String {
        match self {
            InconclusiveCause::Infra(c) => format!("infra:{}", c.as_str()),
            InconclusiveCause::VerifierUnavailable(c) => {
                format!("verifier_unavailable:{c}")
            }
            InconclusiveCause::BackendTerminated => "backend_terminated".to_string(),
            InconclusiveCause::Teardown => "teardown".to_string(),
            InconclusiveCause::Cancelled => "cancelled".to_string(),
        }
    }

    /// Parse the inside of `inconclusive{…}`; `Err` otherwise.
    pub fn parse(s: &str) -> Result<InconclusiveCause, VocabularyError> {
        if let Some(c) = s.strip_prefix("infra:") {
            return InfraCause::parse(c).map(InconclusiveCause::Infra);
        }
        if let Some(c) = s.strip_prefix("verifier_unavailable:") {
            if c.is_empty() || c.contains('{') || c.contains('}') {
                return Err(refuse("inconclusive_cause", s));
            }
            return Ok(InconclusiveCause::VerifierUnavailable(c.to_string()));
        }
        Ok(match s {
            "backend_terminated" => InconclusiveCause::BackendTerminated,
            "teardown" => InconclusiveCause::Teardown,
            "cancelled" => InconclusiveCause::Cancelled,
            _ => return Err(refuse("inconclusive_cause", s)),
        })
    }
}

// ── Verdict flags (Appendix D "Flags" row) ──────────────────────────────────

/// The reserved verdict/slot flag spellings — carried verbatim, never
/// relabelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SlotFlag {
    /// `relay_killed_by_agent` — relay death after an agent-issued signal
    /// to PID 1 (REQ-XB-129): `scored` with `end_cause: crashed`, never
    /// `infrastructure_failure`.
    RelayKilledByAgent,
    /// `reward_ctrf_inconsistent`.
    RewardCtrfInconsistent,
    /// `infra_suspected:verifier_no_report`.
    InfraSuspectedVerifierNoReport,
    /// `inconclusive_with_receipt`.
    InconclusiveWithReceipt,
    /// `harness_self_modified`.
    HarnessSelfModified,
    /// `undeclared_egress_denied`.
    UndeclaredEgressDenied,
    /// `boundary_ambiguous`.
    BoundaryAmbiguous,
    /// `reservation_bound_violated`.
    ReservationBoundViolated,
    /// `drift_observed`.
    DriftObserved,
    /// `allowlist_extended`.
    AllowlistExtended,
    /// `archive_verified`.
    ArchiveVerified,
}

impl SlotFlag {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SlotFlag::RelayKilledByAgent => "relay_killed_by_agent",
            SlotFlag::RewardCtrfInconsistent => "reward_ctrf_inconsistent",
            SlotFlag::InfraSuspectedVerifierNoReport => "infra_suspected:verifier_no_report",
            SlotFlag::InconclusiveWithReceipt => "inconclusive_with_receipt",
            SlotFlag::HarnessSelfModified => "harness_self_modified",
            SlotFlag::UndeclaredEgressDenied => "undeclared_egress_denied",
            SlotFlag::BoundaryAmbiguous => "boundary_ambiguous",
            SlotFlag::ReservationBoundViolated => "reservation_bound_violated",
            SlotFlag::DriftObserved => "drift_observed",
            SlotFlag::AllowlistExtended => "allowlist_extended",
            SlotFlag::ArchiveVerified => "archive_verified",
        }
    }

    /// Every member — the closed set.
    pub const ALL: [SlotFlag; 11] = [
        SlotFlag::RelayKilledByAgent,
        SlotFlag::RewardCtrfInconsistent,
        SlotFlag::InfraSuspectedVerifierNoReport,
        SlotFlag::InconclusiveWithReceipt,
        SlotFlag::HarnessSelfModified,
        SlotFlag::UndeclaredEgressDenied,
        SlotFlag::BoundaryAmbiguous,
        SlotFlag::ReservationBoundViolated,
        SlotFlag::DriftObserved,
        SlotFlag::AllowlistExtended,
        SlotFlag::ArchiveVerified,
    ];

    /// Parse; `Err` on any other input.
    pub fn parse(s: &str) -> Result<SlotFlag, VocabularyError> {
        Self::ALL
            .iter()
            .find(|f| f.as_str() == s)
            .copied()
            .ok_or_else(|| refuse("slot_flag", s))
    }
}

// ── The slot disposition (§3.3 + design/31 §1.4) ─────────────────────────────

/// `disposition` — the slot's typed outcome spelling: `executed{resolved |
/// unresolved}` with `end_cause`, `inconclusive{<cause>}`,
/// `withheld{<reason>}`, `failed{<reason>}`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Disposition {
    /// `executed{resolved|unresolved}` — a valid reward was recorded; the
    /// `end_cause` reports how the agent phase ended.
    Executed {
        /// `true` = `resolved`; `false` = `unresolved`.
        resolved: bool,
        /// The agent-phase end cause.
        end_cause: EndCause,
    },
    /// `inconclusive{<cause>}` — infrastructure, verifier or control
    /// endings; the attempt is preserved, never fabricated.
    Inconclusive {
        /// The cause.
        cause: InconclusiveCause,
    },
    /// `withheld{<reason>}` — never ran (or fields sealed); the reason is
    /// the closed member set.
    Withheld {
        /// The reason.
        reason: WithheldReason,
    },
    /// `failed{<reason>}` — a declared failure (Appendix D).
    Failed {
        /// The declared reason (non-empty, no braces).
        reason: String,
    },
}

impl Disposition {
    /// The canonical spelling.
    pub fn as_str(&self) -> String {
        match self {
            Disposition::Executed { resolved, .. } => {
                if *resolved {
                    "executed{resolved}".to_string()
                } else {
                    "executed{unresolved}".to_string()
                }
            }
            Disposition::Inconclusive { cause } => format!("inconclusive{{{}}}", cause.as_str()),
            Disposition::Withheld { reason } => format!("withheld{{{}}}", reason.as_str()),
            Disposition::Failed { reason } => format!("failed{{{reason}}}"),
        }
    }

    /// The `end_cause` when the disposition is `executed`.
    pub fn end_cause(&self) -> Option<EndCause> {
        match self {
            Disposition::Executed { end_cause, .. } => Some(*end_cause),
            _ => None,
        }
    }

    /// Parse the canonical spelling; `Err` outside the vocabulary.
    pub fn parse(s: &str) -> Result<Disposition, VocabularyError> {
        if let Some(inner) = s
            .strip_prefix("executed{")
            .and_then(|s| s.strip_suffix('}'))
        {
            return match inner {
                "resolved" => Ok(Disposition::Executed {
                    resolved: true,
                    end_cause: EndCause::Exited(0),
                }),
                "unresolved" => Ok(Disposition::Executed {
                    resolved: false,
                    end_cause: EndCause::Exited(0),
                }),
                _ => Err(refuse("disposition", s)),
            };
        }
        if let Some(inner) = s
            .strip_prefix("inconclusive{")
            .and_then(|s| s.strip_suffix('}'))
        {
            return InconclusiveCause::parse(inner)
                .map(|cause| Disposition::Inconclusive { cause });
        }
        if let Some(inner) = s
            .strip_prefix("withheld{")
            .and_then(|s| s.strip_suffix('}'))
        {
            return WithheldReason::parse(inner).map(|reason| Disposition::Withheld { reason });
        }
        if let Some(inner) = s.strip_prefix("failed{").and_then(|s| s.strip_suffix('}')) {
            if inner.is_empty() {
                return Err(refuse("disposition", s));
            }
            return Ok(Disposition::Failed {
                reason: inner.to_string(),
            });
        }
        Err(refuse("disposition", s))
    }

    /// The canonical JSON (a string).
    pub fn to_json(&self) -> Json {
        Json::str(self.as_str())
    }

    /// Strict decode — `Err` on non-strings and non-members.
    pub fn from_json(j: &Json) -> Result<Disposition, VocabularyError> {
        match j.as_str() {
            Some(s) => Disposition::parse(s),
            None => Err(refuse("disposition", "<non-string>")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn withheld_member_set_is_the_closed_spec_set() {
        // The Appendix D + contract-Notes enumeration — a member outside
        // this set refuses.
        for (r, spelling) in [
            (WithheldReason::Budget, "budget"),
            (WithheldReason::WindowDeadline, "window_deadline"),
            (WithheldReason::ImageDrift, "image_drift"),
            (WithheldReason::DatasetPrivate, "dataset_private"),
            (WithheldReason::NoCandidate, "no_candidate"),
            (WithheldReason::InertPredicted, "inert_predicted"),
            (
                WithheldReason::InertAtAdmissibleWindow,
                "inert_at_admissible_window",
            ),
            (WithheldReason::Infra, "infra"),
            (WithheldReason::L1NotPassed, "l1_not_passed"),
            (WithheldReason::SealedUntilResults, "sealed_until_results"),
            (
                WithheldReason::Admission(AdmissionReason::Gate(3)),
                "admission:AG3",
            ),
            (
                WithheldReason::Admission(AdmissionReason::HarnessLayerMismatch),
                "admission:harness_layer_mismatch",
            ),
            (
                WithheldReason::Admission(AdmissionReason::NativeLaneHold),
                "admission:native_lane_hold",
            ),
            (
                WithheldReason::Admission(AdmissionReason::Pp0InfraHold),
                "admission:pp0_infra_hold",
            ),
            (
                WithheldReason::Admission(AdmissionReason::TaskDigestMismatch),
                "admission:task_digest_mismatch",
            ),
        ] {
            assert_eq!(r.as_str(), spelling);
            assert_eq!(WithheldReason::parse(spelling).unwrap(), r);
        }
        assert_eq!(WithheldReason::NON_ADMISSION.len(), 10);
        for bad in [
            "fallback",
            "end_after_w2",
            "declined",
            "AG0",
            "admission:AG0",
            "admission:AG10",
            "admission:",
            "",
            "withheld",
            "sealed",
        ] {
            assert!(WithheldReason::parse(bad).is_err(), "{bad}");
        }
        for n in 1..=9u8 {
            let s = format!("AG{n}");
            assert_eq!(
                AdmissionReason::parse(&s).unwrap(),
                AdmissionReason::Gate(n)
            );
        }
        assert!(AdmissionReason::parse("AG0").is_err());
        assert!(AdmissionReason::parse("AG10").is_err());
    }

    #[test]
    fn disposition_spelling_round_trips() {
        let cases = [
            "executed{resolved}",
            "executed{unresolved}",
            "inconclusive{infra:runner_crash}",
            "inconclusive{infra:provision:podman_pull}",
            "inconclusive{infra:budget_reservation}",
            "inconclusive{verifier_unavailable:reward_missing}",
            "inconclusive{backend_terminated}",
            "inconclusive{teardown}",
            "inconclusive{cancelled}",
            "withheld{admission:task_digest_mismatch}",
            "withheld{budget}",
            "failed{declared}",
        ];
        for s in cases {
            assert_eq!(Disposition::parse(s).unwrap().as_str(), s);
        }
        for bad in ["executed{maybe}", "declined", "inconclusive{}", "n/a{x}"] {
            assert!(Disposition::parse(bad).is_err(), "{bad}");
        }
    }
}
