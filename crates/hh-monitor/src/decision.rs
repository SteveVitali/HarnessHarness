//! `KernelDecision` — the §5g.1 §3 record `authorize` produces and
//! `security.permission.decided` records (ADR-0052 D1; ADR-0031's shape
//! extended). The Stage-1 record carries the decision, the check trail and the
//! inputs the decision was computed over; `assessment_inputs_ref`, `cache_key`
//! and `remedy_taken` are Stage-2 fields — honestly absent, never stubbed.

use std::collections::BTreeSet;

use hh_ontology::risk::RiskClass;
use hh_provenance::{AuthorityClass, TaintTag};
use hh_wire::json::Json;

/// `decision ∈ {allow, ask{options, remedies[]}, deny{reason, remedies[]}}`
/// (ADR-0052 D1).
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// The effect may commit.
    Allow,
    /// A human must decide — carries the option kinds and remedy refs.
    Ask {
        /// The `ApprovalOption.kind` subset offered (ADR-0070 D1 sum; CF-480 —
        /// the ACP-lowered spellings never appear here).
        options: Vec<String>,
        /// Remedy refs (ADR-0055 D5 — C2 renders them; Stage 1 carries the
        /// closed reason only).
        remedies: Vec<String>,
    },
    /// Refused — carries the closed `DenyReason`.
    Deny {
        /// The typed reason.
        reason: DenyReason,
        /// Remedy refs.
        remedies: Vec<String>,
    },
}

impl Decision {
    /// The decision tag spelling used in the `decided` payload.
    pub fn tag(&self) -> &'static str {
        match self {
            Decision::Allow => "allow",
            Decision::Ask { .. } => "ask",
            Decision::Deny { .. } => "deny",
        }
    }
}

/// `DenyReason` — the closed sum (ADR-0052 D1; CF-193; ADR-0062 D3). Growth is
/// a dialect bump, never a stringly-typed detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyReason {
    /// No `ProvenanceRecord` on the proposal.
    MissingProvenance,
    /// A surface argument absent from the `SurfaceArgMap` (I-H5).
    UnmappedArgument,
    /// A scope-bearing parameter absent from `scope_bindings` (ADR-0087 D3).
    UnscopedParameter,
    /// The proposer is not a registered `AgentProcess`.
    UnknownProposer,
    /// No live handle grants cover the effect's domain + scope.
    NoCoveringGrant,
    /// A covering grant's `count`/`time`/`budget` constraint is exhausted.
    GrantConstraintExhausted,
    /// The covering handle is revoked or expired.
    HandleRevoked,
    /// Π denied the proposal.
    PolicyDenied,
    /// The effect's persistence/persistence-scope ceiling was exceeded
    /// (check 5 — Stage 2; declared now so the sum is the closed one).
    ScopeCeilingExceeded,
    /// A `delegate`/`spawn` request exceeds the parent's grants/ceiling.
    AuthorityWidening,
    /// The parent handle is not `delegable`.
    NotDelegable,
    /// The requested budget exceeds the parent's (step 6 sibling).
    BudgetExceedsParent,
    /// The approvals budget cannot reserve the `ask` (step 7 — Stage 2).
    ApprovalsExhausted,
    /// `unattended` mode: `ask` defaults to deny (Π-12).
    UnattendedAsk,
    /// An approval window elapsed (Stage 2).
    ApprovalTimedOut,
    /// The containment precondition is unverified (ADR-0062 D3 — R-2.8.4).
    ContainmentUnverified,
    /// The `containment` catch-all member of the closed sum (CF-193).
    Containment,
}

impl DenyReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DenyReason::MissingProvenance => "MissingProvenance",
            DenyReason::UnmappedArgument => "UnmappedArgument",
            DenyReason::UnscopedParameter => "UnscopedParameter",
            DenyReason::UnknownProposer => "UnknownProposer",
            DenyReason::NoCoveringGrant => "NoCoveringGrant",
            DenyReason::GrantConstraintExhausted => "GrantConstraintExhausted",
            DenyReason::HandleRevoked => "HandleRevoked",
            DenyReason::PolicyDenied => "PolicyDenied",
            DenyReason::ScopeCeilingExceeded => "ScopeCeilingExceeded",
            DenyReason::AuthorityWidening => "AuthorityWidening",
            DenyReason::NotDelegable => "NotDelegable",
            DenyReason::BudgetExceedsParent => "BudgetExceedsParent",
            DenyReason::ApprovalsExhausted => "ApprovalsExhausted",
            DenyReason::UnattendedAsk => "UnattendedAsk",
            DenyReason::ApprovalTimedOut => "ApprovalTimedOut",
            DenyReason::ContainmentUnverified => "ContainmentUnverified",
            DenyReason::Containment => "containment",
        }
    }
}

/// `decider ∈ {policy, cache, hook, auto_reviewer, human}` — the one closed sum
/// (CF-153/CF-312; `pre_authorization` is retired). Only `policy` is *produced*
/// at Stage 1 — the other members exist so the recorded sum is complete and a
/// later stage never renames the tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decider {
    /// The Π interpreter.
    Policy,
    /// An approval lease (R-2.8.7 — Stage 2).
    Cache,
    /// An admitted deterministic hook (R-2.8.5 — raise/deny only).
    Hook,
    /// The auto-reviewer (Π-12 `auto_review` — Stage 2).
    AutoReviewer,
    /// The human principal's approval (Stage 2).
    Human,
}

impl Decider {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Decider::Policy => "policy",
            Decider::Cache => "cache",
            Decider::Hook => "hook",
            Decider::AutoReviewer => "auto_reviewer",
            Decider::Human => "human",
        }
    }
}

/// One step of the decision — the `checks[]` entry that makes the reason
/// reconstructible (ADR-0066 D5: the reason is derivable from the record).
#[derive(Debug, Clone, PartialEq)]
pub struct CheckRecord {
    /// The step number (0–3, 6 at Stage 1; the sum keeps the spec's numbering).
    pub step: u8,
    /// `pass` | `fail` | `n/a` — the step's outcome.
    pub outcome: &'static str,
    /// The closed detail tag (never prose — a `DenyReason` spelling or a row id
    /// like `pi_7`; refusal text is profile-rendered from this, ADR-0052 D3).
    pub detail: String,
}

/// `KernelDecision` — `{effect_id, decision, effective_authority, taint,
/// effective_risk_class, handle_ids[], policy_ref, checks[], decider}`
/// (§5g.1 §3; the Stage-2 members `assessment_inputs_ref`, `cache_key`,
/// `remedy_taken` are absent — not nulled).
#[derive(Debug, Clone, PartialEq)]
pub struct KernelDecision {
    /// The effect the decision is for.
    pub effect_id: String,
    /// The decision.
    pub decision: Decision,
    /// `min(authority(proposer), context_label.authority)` — step 1.
    pub effective_authority: AuthorityClass,
    /// `taint(context_label) ∪ taint(args)` — step 1.
    pub taint: BTreeSet<TaintTag>,
    /// `max_by_danger(kernel_assessed, projected(declared), self_report)` —
    /// step 3 (raise-only).
    pub effective_risk_class: RiskClass,
    /// The covering handle ids the decision relied on.
    pub handle_ids: Vec<String>,
    /// The Π table version the decision was computed under.
    pub policy_ref: String,
    /// The check trail.
    pub checks: Vec<CheckRecord>,
    /// Who decided — `policy` for every Stage-1 decision.
    pub decider: Decider,
}

impl KernelDecision {
    /// The `security.permission.decided` payload members the gate fold reads
    /// plus the full decision record (the event builder is [`crate::events`]).
    pub fn to_json(&self) -> Json {
        let mut m = vec![
            ("effect_id", Json::str(self.effect_id.clone())),
            ("decision", Json::str(self.decision.tag())),
            (
                "effective_authority",
                Json::str(self.effective_authority.as_str()),
            ),
            ("effective_risk_class", self.effective_risk_class.to_json()),
            (
                "handle_ids",
                Json::Arr(
                    self.handle_ids
                        .iter()
                        .map(|h| Json::str(h.clone()))
                        .collect(),
                ),
            ),
            ("policy_ref", Json::str(self.policy_ref.clone())),
            ("decider", Json::str(self.decider.as_str())),
        ];
        match &self.decision {
            Decision::Ask { options, remedies } => {
                m.push((
                    "options",
                    Json::Arr(options.iter().map(|o| Json::str(o.clone())).collect()),
                ));
                m.push((
                    "remedies",
                    Json::Arr(remedies.iter().map(|r| Json::str(r.clone())).collect()),
                ));
            }
            Decision::Deny { reason, remedies } => {
                m.push(("reason", Json::str(reason.as_str())));
                m.push((
                    "remedies",
                    Json::Arr(remedies.iter().map(|r| Json::str(r.clone())).collect()),
                ));
            }
            Decision::Allow => {}
        }
        Json::Obj(m.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
}
