//! `KernelDecision` — the §5g.1 §3 record `authorize` produces and
//! `security.permission.decided` records (ADR-0052 D1; ADR-0031's shape
//! extended). The Stage-1 record carries the decision, the check trail and the
//! inputs the decision was computed over; `assessment_inputs_ref`, `cache_key`
//! and `remedy_taken` are Stage-2 fields — honestly absent, never stubbed.

use std::collections::BTreeSet;

use hh_ontology::risk::RiskClass;
use hh_provenance::flow::{EnforcementClass, Remedy};
use hh_provenance::{AuthorityClass, TaintTag};
use hh_wire::json::Json;

/// `decision ∈ {allow, ask{options, remedies[]}, deny{reason, remedies[]}}`
/// (ADR-0052 D1). `remedies` carries the closed `Remedy` sum (§5g.2 §3;
/// ADR-0055 D5) — the bounded, deterministic set C2 enumerates.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// The effect may commit.
    Allow,
    /// A human must decide — carries the option kinds and remedy refs.
    Ask {
        /// The `ApprovalOption.kind` subset offered (ADR-0070 D1 sum; CF-480 —
        /// the ACP-lowered spellings never appear here).
        options: Vec<String>,
        /// The closed `Remedy` set offered (ADR-0055 D5 — R1–R5 bounded,
        /// narrowing-or-endorsed only).
        remedies: Vec<Remedy>,
    },
    /// Refused — carries the closed `DenyReason`.
    Deny {
        /// The typed reason.
        reason: DenyReason,
        /// The closed `Remedy` set offered.
        remedies: Vec<Remedy>,
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
    /// D-ROBUST (I-F2; §5g.2): an endorsement/declassification decision's
    /// input (a `recipient_params` value, a sanitizer selector argument, an
    /// approval `subject_ref`) carries `authority ≤ external` or `taint ≠ ∅`
    /// and was not itself shape-endorsed — refused before any `ask`.
    RobustnessViolated,
    /// Check 3 (I-F4; §5g.2): `recipients(p) ⊄ readers(x)` for a
    /// `content_param` — the egress's declared readers do not cover the
    /// resolved recipients.
    ReaderCoverage,
    /// A flow-condition atom could not produce a definite verdict (an
    /// unbound `arg`, a missing recorded detector verdict, a malformed
    /// `flow_contract` on the registered capability) — the closed grammar's
    /// totality makes the failure a `deny` (§5g.2 §5).
    EvaluationError,
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
            DenyReason::RobustnessViolated => "RobustnessViolated",
            DenyReason::ReaderCoverage => "ReaderCoverage",
            DenyReason::EvaluationError => "EvaluationError",
        }
    }
}

/// `decision_scope ∈ {once, session, persisted}` — the §5g.6 §6.1 dossier's
/// closed sum on `security.permission.decided`/`granted`. `once` is a
/// single-attempt decision; `session` is in force for the run (the root-handle
/// lifetime, `HandleExpiry::Run`); `persisted` is a widening that survives the
/// run and so requires the human-origin `lifecycle.definition.changed` rule
/// change (ADR-0066 D5 `persisted_widening`). Only `once` is *produced* by
/// `authorize` at Stage 1 — the other members exist so the recorded sum is
/// complete and a later stage never renames the tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionScope {
    /// One attempt cycle.
    Once,
    /// In force for the run.
    Session,
    /// In force beyond the run — a rule change.
    Persisted,
}

impl DecisionScope {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DecisionScope::Once => "once",
            DecisionScope::Session => "session",
            DecisionScope::Persisted => "persisted",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<DecisionScope> {
        match s {
            "once" => Some(DecisionScope::Once),
            "session" => Some(DecisionScope::Session),
            "persisted" => Some(DecisionScope::Persisted),
            _ => None,
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
    /// The check's `EnforcementClass` (I-F1 — every check carries it;
    /// `deterministic` for the §5g.1 steps, the contract's own class for the
    /// flow stage).
    pub enforcement: EnforcementClass,
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
    /// `decision_scope` — `once` for every Stage-1 `authorize` decision (the
    /// `session`/`persisted` grant machinery lands with approvals); declared so
    /// the audit row carries the closed sum, never an implicit default.
    pub decision_scope: DecisionScope,
    /// `cache_key` — the lease key that served a `decider = cache` allow
    /// (step 8; §5g.7 I-P6). `None` on every non-cache decision.
    pub cache_key: Option<String>,
    /// `origin_permission_id` — the pending the serving lease descends from
    /// (step 8's `decider = cache` allow carries it; AC-R-2.8.7-4).
    pub origin_permission_id: Option<String>,
    /// `assessment_inputs_ref` — the recorded-inputs coordinate (the
    /// `AssessmentInputs` canonical hash — the reason is reconstructible,
    /// ADR-0066 D5).
    pub assessment_inputs_ref: Option<String>,
    /// `remedy_taken` — the `Remedy` a prior decision stage consumed on this
    /// `effect_id` (`Some` only when a recorded remedy was applied — the
    /// dispatcher stamps it; §5g.2 §3 `decided{remedy_taken?}`).
    pub remedy_taken: Option<Remedy>,
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
            ("decision_scope", Json::str(self.decision_scope.as_str())),
            (
                "checks",
                Json::Arr(
                    self.checks
                        .iter()
                        .map(|c| {
                            Json::obj([
                                ("step", Json::Int(c.step as i64)),
                                ("outcome", Json::str(c.outcome)),
                                ("detail", Json::str(c.detail.clone())),
                                ("enforcement", Json::str(c.enforcement.as_str())),
                            ])
                        })
                        .collect(),
                ),
            ),
        ];
        // Stage-2 members are *absent*, never nulled (CC8 — a Stage-1 row
        // decodes identically).
        if let Some(k) = &self.cache_key {
            m.push(("cache_key", Json::str(k.clone())));
        }
        if let Some(k) = &self.origin_permission_id {
            m.push(("origin_permission_id", Json::str(k.clone())));
        }
        if let Some(k) = &self.assessment_inputs_ref {
            m.push(("assessment_inputs_ref", Json::str(k.clone())));
        }
        if let Some(r) = &self.remedy_taken {
            m.push(("remedy_taken", r.to_json()));
        }
        match &self.decision {
            Decision::Ask { options, remedies } => {
                m.push((
                    "options_presented",
                    Json::Arr(options.iter().map(|o| Json::str(o.clone())).collect()),
                ));
                m.push((
                    "remedies",
                    Json::Arr(remedies.iter().map(|r| r.to_json()).collect()),
                ));
            }
            Decision::Deny { reason, remedies } => {
                m.push(("reason", Json::str(reason.as_str())));
                m.push((
                    "remedies",
                    Json::Arr(remedies.iter().map(|r| r.to_json()).collect()),
                ));
            }
            Decision::Allow => {}
        }
        Json::Obj(m.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
}
