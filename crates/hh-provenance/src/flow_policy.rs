//! `FlowPolicy` — the `HarnessRule{action: flow_policy(FlowPolicy)}` payload
//! shape §5g.2 §3 owns (ADR-0056 D1): `{policy_id, issuer: ProvenanceRecord,
//! scope: PersistenceScope, rules: [FlowRule]}` issued at `authority ≥
//! definition`. `hh-hir` carries the action as a structured record
//! (`RuleAction::FlowPolicy(Json)`); this module owns the member shape, the
//! seal-time validation, evaluation through [`check_flow`], and
//! [`classify_policy_edit`] (ADR-0056 D5).
//!
//! # Classification (ADR-0056 D5)
//!
//! `classify_policy_edit(P, P′, space) → narrowing | widening | incomparable`
//! is **exact** over the caller-declared finite [`ProposalSpace`]:
//! `narrowing ⇔ ∀p ∈ space. allowed(P′, p) ⇒ allowed(P, p)` where `allowed`
//! is the real [`check_flow`] verdict (`allow`/`declassified` — an `ask`,
//! `deny` or `fallthrough` is never an authorization). The finite space is
//! the decidable encoding ADR-0056 D5 permits ("a sound and complete
//! encoding into a decidable logic or an equivalent syntactic subsumption"):
//! the grammar is closed and total, so enumeration over the declared space
//! *is* the sound-and-complete check — the AC-R-2.8.2-9 agreement test
//! re-derives the same classification by independent exhaustive enumeration.
//! `incomparable` is treated as `widening` by [`apply_policy_edit`].
//!
//! # Application
//!
//! `apply_policy_edit(P, P′, proposer, space)` implements the §5g.2 §5 row +
//! ADR-0017's `authority_delta` feed: a `delegate`-class or `evolution`-origin
//! proposer may apply only `narrowing`; `widening`/`incomparable` requires a
//! `human`/`kernel`/`definition` origin (the principal's edit channel). The
//! refusal is typed — never a silent rejection.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_wire::json::Json;

use crate::authority::{AuthorityClass, PersistenceScope};
use crate::decode::DecodeError;
use crate::flow::{
    check_flow, flow_rule_from_json, flow_rule_json, CommittedEffect, EvalError, FlowInput,
    FlowRule, FlowVerdict,
};
use crate::label::Label;
use crate::origin::Origin;
use crate::record::ProvenanceRecord;

/// The bound on rules per policy (the grammar is bounded — I-F7's "bounded"
/// is made concrete here; a policy is MUST-data and its evaluation is linear
/// in `|rules|`).
pub const FLOW_POLICY_MAX_RULES: usize = 1024;

/// `FlowPolicy{policy_id, issuer, scope, rules}` (§5g.2 §3 row; ADR-0056 D1).
/// MUST-data — carried on `HarnessRule{action: flow_policy}`; the evaluator
/// and classifier are MUST-code.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowPolicy {
    /// The policy's identity coordinate.
    pub policy_id: String,
    /// The issuing record — `authority ≥ definition` enforced by
    /// [`validate_flow_policy`]; a `delegate`-class issuer never validates.
    pub issuer: ProvenanceRecord,
    /// The persistence scope the policy applies under.
    pub scope: PersistenceScope,
    /// The flow rules — the same closed grammar `FlowContract.rules` uses.
    pub rules: Vec<FlowRule>,
}

/// The seal-time policy failures (every refusal typed — CC3).
#[derive(Debug, Clone, PartialEq)]
pub enum FlowPolicyError {
    /// `issuer.authority < definition` — a flow policy is issued at
    /// `definition` or above; a `delegate`-class issuer is illegitimate.
    IllegitimateIssuer {
        /// The issuer's authority class.
        authority: AuthorityClass,
    },
    /// The rule list exceeds [`FLOW_POLICY_MAX_RULES`].
    Unbounded {
        /// The declared rule count.
        rules: usize,
    },
    /// A rule id repeats — the check trail's row spelling must be unique.
    DuplicateRuleId {
        /// The duplicated id.
        rule_id: String,
    },
    /// A `deny` rule's `reason` is not a `DenyReason` spelling the monitor
    /// maps (the closed `DenyReason` sum — the monitor's
    /// `deny_reason_spelling` is the authoritative list; the policy layer
    /// checks non-empty + the recorded set below).
    UnknownDenyReason {
        /// The rule carrying it.
        rule_id: String,
        /// The unrecognized spelling.
        reason: String,
    },
    /// A `declassify`/`sanitize` decision names an empty ref or param — a
    /// malformed target is a schema violation, never ignored.
    MalformedDecision {
        /// The rule carrying it.
        rule_id: String,
        /// What was malformed.
        detail: String,
    },
}

/// `validate_flow_policy` — the seal-time checks over a decoded policy
/// (§5g.2 §5: "a rule that recurses, embeds code, or errors on evaluation is
/// refused at `seal`"). Recursion and embedded code are already impossible
/// *by grammar* — the [`FlowRule`]/[`crate::flow::FlowCond`] codecs are
/// closed-member and fail-closed, conditions are quantifier-free over the
/// closed atom set with `COND_MAX_DEPTH`/`COND_MAX_NODES` bounds, and the
/// only iteration is the built-in `∀` over params/committed effects (I-F7).
/// This pass enforces the record-level invariants the codec cannot see:
/// issuer authority, boundedness, unique rule ids, closed `DenyReason`
/// spellings, and well-formed decision targets.
pub fn validate_flow_policy(policy: &FlowPolicy) -> Result<(), FlowPolicyError> {
    if policy.issuer.authority < AuthorityClass::Definition {
        return Err(FlowPolicyError::IllegitimateIssuer {
            authority: policy.issuer.authority,
        });
    }
    if policy.rules.len() > FLOW_POLICY_MAX_RULES {
        return Err(FlowPolicyError::Unbounded {
            rules: policy.rules.len(),
        });
    }
    let mut ids = BTreeSet::new();
    for r in &policy.rules {
        if !ids.insert(r.rule_id.clone()) {
            return Err(FlowPolicyError::DuplicateRuleId {
                rule_id: r.rule_id.clone(),
            });
        }
        match &r.decision {
            crate::flow::FlowDecision::Deny { reason } => {
                if !DENY_REASON_SPELLINGS.contains(&reason.as_str()) {
                    return Err(FlowPolicyError::UnknownDenyReason {
                        rule_id: r.rule_id.clone(),
                        reason: reason.clone(),
                    });
                }
            }
            crate::flow::FlowDecision::Declassify { readers_to } => {
                if let crate::flow::ReadersTo::Named(rs) = readers_to {
                    if rs.is_empty() || rs.iter().any(|s| s.is_empty()) {
                        return Err(FlowPolicyError::MalformedDecision {
                            rule_id: r.rule_id.clone(),
                            detail: "declassify readers_to names an empty set/member".into(),
                        });
                    }
                }
            }
            crate::flow::FlowDecision::Sanitize {
                sanitizer_ref,
                param,
            } => {
                if sanitizer_ref.is_empty() || param.is_empty() {
                    return Err(FlowPolicyError::MalformedDecision {
                        rule_id: r.rule_id.clone(),
                        detail: "sanitize requires non-empty sanitizer_ref and param".into(),
                    });
                }
            }
            crate::flow::FlowDecision::Allow => {}
        }
    }
    Ok(())
}

/// The `DenyReason` spellings a `deny` rule may name — the closed sum the
/// monitor maps (`deny_reason_spelling`/`DenyReason::as_str` — the canonical
/// spellings); kept in one list here so a policy authored against a
/// misspelled reason fails at `seal`, never at eval.
pub const DENY_REASON_SPELLINGS: &[&str] = &[
    "MissingProvenance",
    "UnmappedArgument",
    "UnscopedParameter",
    "UnknownProposer",
    "NoCoveringGrant",
    "GrantConstraintExhausted",
    "HandleRevoked",
    "PolicyDenied",
    "ScopeCeilingExceeded",
    "AuthorityWidening",
    "NotDelegable",
    "BudgetExceedsParent",
    "ApprovalsExhausted",
    "UnattendedAsk",
    "ApprovalTimedOut",
    "ContainmentUnverified",
    "containment",
    "RobustnessViolated",
    "ReaderCoverage",
    "EvaluationError",
];

impl FlowPolicy {
    /// The canonical JSON — `{policy_id, issuer, scope, rules}` (closed
    /// member set; the codec fails on any unknown member).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("policy_id", Json::str(self.policy_id.clone())),
            ("issuer", self.issuer.to_json()),
            ("scope", Json::str(self.scope.as_str())),
            (
                "rules",
                Json::Arr(self.rules.iter().map(flow_rule_json).collect()),
            ),
        ])
    }

    /// Parse the canonical form — closed members, fail-closed (a misspelled
    /// or extra member is a schema violation, never ignored — CC3). This is
    /// the "rejects a recursive or code-bearing rule at `seal`" half of
    /// AC-R-2.8.2-8: the grammar admits no recursion or code-bearing member
    /// by construction; anything else is a decode error.
    pub fn from_json(j: &Json) -> Result<FlowPolicy, DecodeError> {
        let err = |m: String| DecodeError {
            detail: format!("flow_policy.{m}"),
        };
        let Json::Obj(m) = j else {
            return Err(err("must be an object".to_string()));
        };
        for k in m.keys() {
            match k.as_str() {
                "policy_id" | "issuer" | "scope" | "rules" => {}
                other => return Err(err(format!("unknown member {other}"))),
            }
        }
        let policy_id = j
            .get("policy_id")
            .and_then(Json::as_str)
            .ok_or_else(|| err("policy_id missing".to_string()))?;
        if policy_id.is_empty() {
            return Err(err("policy_id empty".to_string()));
        }
        let issuer = ProvenanceRecord::from_json(
            j.get("issuer")
                .ok_or_else(|| err("issuer missing".to_string()))?,
        )
        .map_err(|e| err(format!("issuer: {}", e.detail)))?;
        let scope = match j.get("scope").and_then(Json::as_str) {
            Some("definition") => PersistenceScope::Definition,
            Some("user") => PersistenceScope::User,
            Some("project") => PersistenceScope::Project,
            Some("session") => PersistenceScope::Session,
            Some("run") => PersistenceScope::Run,
            Some("turn") => PersistenceScope::Turn,
            Some(other) => return Err(err(format!("scope {other} unknown"))),
            None => return Err(err("scope missing".to_string())),
        };
        let rules = match j.get("rules") {
            Some(Json::Arr(items)) => items
                .iter()
                .enumerate()
                .map(|(i, r)| flow_rule_from_json(r, &format!("flow_policy.rules[{i}]")))
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => return Err(err("rules must be an array".to_string())),
            None => Vec::new(),
        };
        Ok(FlowPolicy {
            policy_id: policy_id.to_string(),
            issuer,
            scope,
            rules,
        })
    }
}

/// Evaluate a validated policy's rules through the shared [`check_flow`]
/// evaluator (one evaluator — the `FlowContract` and `FlowPolicy` rule lists
/// run the same tier order: all `deny` → `allow`/`declassify`/`sanitize` →
/// fallthrough to Π's default row).
pub fn check_flow_policy(policy: &FlowPolicy, input: &FlowInput) -> Result<FlowVerdict, EvalError> {
    check_flow(&policy.rules, input)
}

// ── PolicyEditClass + the finite ProposalSpace (ADR-0056 D5) ─────────────────

/// `classify_policy_edit`'s three-valued result (`narrowing | widening |
/// incomparable` — `incomparable` is treated as widening by
/// [`apply_policy_edit`], never as narrowing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyEditClass {
    /// `∀p ∈ space. allowed(P′, p) ⇒ allowed(P, p)` — the edit only restricts
    /// (equality included: a no-op edit is a narrowing).
    Narrowing,
    /// `∀p ∈ space. allowed(P, p) ⇒ allowed(P′, p)` with at least one
    /// strictly-new allowance — a pure widening.
    Widening,
    /// Mixed/incomparable — some point newly allowed *and* some point newly
    /// disallowed (or vice versa).
    Incomparable,
}

impl PolicyEditClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            PolicyEditClass::Narrowing => "narrowing",
            PolicyEditClass::Widening => "widening",
            PolicyEditClass::Incomparable => "incomparable",
        }
    }
}

/// One point of the finite proposal space — the concrete `FlowInput` member
/// set the classifier enumerates (§5g.2 §2.3's `p`: effect class, canonical
/// args, per-parameter labels, recipients, `L⁺`, the committed projection
/// and recorded detector verdicts).
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyPoint {
    /// The declared effect domain spelling.
    pub domain: String,
    /// The declared world (`open`/`closed`).
    pub world: String,
    /// The capability's semantic id (selector + `{tool}` instantiation).
    pub capability: String,
    /// Canonical parameter path → bound value.
    pub args: BTreeMap<String, Json>,
    /// Canonical parameter path → label.
    pub param_labels: BTreeMap<String, Label>,
    /// `recipients(p)` — the resolved recipient set.
    pub recipients: BTreeSet<String>,
    /// `L⁺(p)` — the prospective label.
    pub l_plus: Label,
    /// `project(run, effects, until_seq)`.
    pub committed: Vec<CommittedEffect>,
    /// Recorded detector verdicts (`"<vref>:<param>" → bool`).
    pub detectors: BTreeMap<String, bool>,
}

impl PolicyPoint {
    /// Borrow as the evaluator's `FlowInput`.
    pub fn as_input(&self) -> FlowInput<'_> {
        FlowInput {
            l_plus: &self.l_plus,
            param_labels: &self.param_labels,
            args: &self.args,
            recipients: &self.recipients,
            domain: &self.domain,
            world: &self.world,
            committed: &self.committed,
            detectors: &self.detectors,
            capability: &self.capability,
        }
    }
}

/// The finite proposal space `classify_policy_edit` enumerates — the
/// caller-declared grid (a suite, a sealed definition's reach, or an
/// exhaustive small domain in a conformance test). `None` detectors member
/// means "no recorded verdicts" — an atom that reads an unrecorded detector
/// is an `EvaluationError` per I-F7, and an erroring policy is `allowed` at
/// **no** point (the fail-closed rule carries into classification).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProposalSpace {
    /// The enumerated points.
    pub points: Vec<PolicyPoint>,
}

/// Whether `rules` allow the flow at `point` — `allow`/`declassified` is an
/// authorization; `deny`/`ask`/`fallthrough` is not; an `EvaluationError` is
/// not (fail-closed — a policy that cannot decide never authorizes).
pub fn rules_allow(rules: &[FlowRule], point: &PolicyPoint) -> bool {
    matches!(
        check_flow(rules, &point.as_input()),
        Ok(FlowVerdict::Allow { .. }) | Ok(FlowVerdict::Declassified { .. })
    )
}

/// Whether `policy` allows the flow at `point` (the [`rules_allow`] row over
/// the policy's rule list).
pub fn policy_allows(policy: &FlowPolicy, point: &PolicyPoint) -> bool {
    rules_allow(&policy.rules, point)
}

/// `classify_policy_edit(P, P′, space) → narrowing | widening | incomparable`
/// — exact over the declared finite [`ProposalSpace`] (ADR-0056 D5):
/// enumerate every point, compare `allowed` sets.
///
/// - `narrowing` — `allowed(P′) ⊆ allowed(P)` (the edit never authorizes
///   anything the old policy didn't; equal sets count);
/// - `widening` — `allowed(P) ⊊ allowed(P′)` (pure superset);
/// - `incomparable` — mixed (some point newly allowed *and* some newly
///   disallowed), or the reverse strict-superset reading fails.
pub fn classify_policy_edit(
    old: &FlowPolicy,
    new: &FlowPolicy,
    space: &ProposalSpace,
) -> PolicyEditClass {
    let mut new_not_old = false;
    let mut old_not_new = false;
    for pt in &space.points {
        let a = policy_allows(old, pt);
        let b = policy_allows(new, pt);
        if b && !a {
            new_not_old = true;
        }
        if a && !b {
            old_not_new = true;
        }
    }
    match (new_not_old, old_not_new) {
        (false, _) => PolicyEditClass::Narrowing,
        (true, false) => PolicyEditClass::Widening,
        (true, true) => PolicyEditClass::Incomparable,
    }
}

/// The policy-edit application refusal — a `delegate`-class or
/// `evolution`-origin proposer may apply only `narrowing` (§5g.2 §5 row;
/// ADR-0017 feeds `authority_delta`).
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyEditError {
    /// The proposer's origin class may not apply a non-narrowing edit — a
    /// widening or incomparable `flow_policy` edit is `human`-origin only
    /// (rejected outright in evolution contexts).
    WideningRequiresHuman {
        /// The classification the edit produced.
        class: PolicyEditClass,
    },
    /// An evolution-context proposer attempted any edit — the §03 invariant
    /// rejects `authority_delta = widening` outright, and a flow-policy edit
    /// by the evolution service is refused before classification reaches it.
    EvolutionOrigin {
        /// The classification the edit produced.
        class: PolicyEditClass,
    },
}

/// `apply_policy_edit(P, P′, proposer, space)` — classify then gate by the
/// proposer's origin (the `delegate`-origin rule of AC-R-2.8.2-9 plus the
/// evolution-context outright rejection of `label::check_diff_authority_delta`):
///
/// - `evolution` origin — refused for any non-narrowing edit
///   (`EvolutionOrigin`; a narrowing edit still applies — an evolution
///   service may restrict, never widen);
/// - other `delegate`-class origins (model, participant, tool) — refused for
///   `widening`/`incomparable` (`WideningRequiresHuman`);
/// - `human`/`kernel`/`definition`-class origins — any class applies.
///
/// Returns the classification on success so the caller can stamp
/// `HirDiff.classification.authority_delta` (`widening` for
/// `Widening`/`Incomparable`).
pub fn apply_policy_edit(
    old: &FlowPolicy,
    new: &FlowPolicy,
    proposer: &Origin,
    space: &ProposalSpace,
) -> Result<PolicyEditClass, PolicyEditError> {
    let class = classify_policy_edit(old, new, space);
    if matches!(proposer, Origin::Evolution { .. }) && class != PolicyEditClass::Narrowing {
        return Err(PolicyEditError::EvolutionOrigin { class });
    }
    if class != PolicyEditClass::Narrowing && proposer.is_delegate_class() {
        return Err(PolicyEditError::WideningRequiresHuman { class });
    }
    Ok(class)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::{FlowDecision, FlowSelector};
    use crate::origin::HumanRole;

    fn issuer() -> ProvenanceRecord {
        let mut r = ProvenanceRecord::minted(
            Origin::human("author", HumanRole::Author),
            PersistenceScope::Definition,
            0,
        );
        r.authority = AuthorityClass::Definition;
        r
    }

    fn rule(id: &str, decision: FlowDecision) -> FlowRule {
        FlowRule {
            rule_id: id.into(),
            selector: FlowSelector {
                domain: None,
                capability: None,
                params: vec![],
            },
            condition: None,
            decision,
            enforcement: crate::flow::EnforcementClass::Deterministic,
            remedies_hint: vec![],
        }
    }

    fn policy(id: &str, rules: Vec<FlowRule>) -> FlowPolicy {
        FlowPolicy {
            policy_id: id.into(),
            issuer: issuer(),
            scope: PersistenceScope::Run,
            rules,
        }
    }

    fn point() -> PolicyPoint {
        PolicyPoint {
            domain: "net_egress".into(),
            world: "open".into(),
            capability: "cap-1".into(),
            args: BTreeMap::new(),
            param_labels: BTreeMap::new(),
            recipients: BTreeSet::new(),
            l_plus: Label::at(AuthorityClass::External),
            committed: vec![],
            detectors: BTreeMap::new(),
        }
    }

    #[test]
    fn codec_round_trips_and_fails_closed() {
        let p = policy(
            "pol-1",
            vec![rule(
                "r1",
                FlowDecision::Deny {
                    reason: "PolicyDenied".into(),
                },
            )],
        );
        let back = FlowPolicy::from_json(&p.to_json()).unwrap();
        assert_eq!(back, p);
        // An unknown member is a schema error — never ignored.
        let mut j = p.to_json();
        if let Json::Obj(m) = &mut j {
            m.insert("exec".into(), Json::str("rm -rf /"));
        }
        assert!(FlowPolicy::from_json(&j).is_err());
        // A code-bearing rule member fails the closed rule codec.
        let mut rj = crate::flow::flow_rule_json(&rule("r1", FlowDecision::Allow));
        if let Json::Obj(m) = &mut rj {
            m.insert("eval".into(), Json::str("system('x')"));
        }
        assert!(crate::flow::flow_rule_from_json(&rj, "t").is_err());
        // A code-bearing condition member likewise.
        let mut cj = Json::obj([("unknown_atom", Json::Bool(true))]);
        if let Json::Obj(m) = &mut cj {
            m.insert("__proto__".into(), Json::Null);
        }
        assert!(crate::flow::FlowCond::from_json(&cj).is_err());
    }

    #[test]
    fn validate_refuses_delegate_issuer_and_duplicates() {
        let mut p = policy("pol-1", vec![]);
        p.issuer = ProvenanceRecord::minted(Origin::model("m", "r", "x"), PersistenceScope::Run, 0);
        assert!(matches!(
            validate_flow_policy(&p),
            Err(FlowPolicyError::IllegitimateIssuer { .. })
        ));
        let dup = policy(
            "pol-2",
            vec![
                rule("r1", FlowDecision::Allow),
                rule(
                    "r1",
                    FlowDecision::Deny {
                        reason: "PolicyDenied".into(),
                    },
                ),
            ],
        );
        assert!(matches!(
            validate_flow_policy(&dup),
            Err(FlowPolicyError::DuplicateRuleId { .. })
        ));
        let bad_reason = policy(
            "pol-3",
            vec![rule(
                "r1",
                FlowDecision::Deny {
                    reason: "not_a_reason".into(),
                },
            )],
        );
        assert!(matches!(
            validate_flow_policy(&bad_reason),
            Err(FlowPolicyError::UnknownDenyReason { .. })
        ));
    }

    fn allow_for(cap: &str) -> FlowPolicy {
        policy(
            "p",
            vec![FlowRule {
                rule_id: "a".into(),
                selector: FlowSelector {
                    domain: None,
                    capability: Some(cap.into()),
                    params: vec![],
                },
                condition: None,
                decision: FlowDecision::Allow,
                enforcement: crate::flow::EnforcementClass::Deterministic,
                remedies_hint: vec![],
            }],
        )
    }

    #[test]
    fn classify_over_the_finite_space_is_exact() {
        // Space: two capabilities. `∅ ⊆ {cap-1} ⊆ {cap-1,cap-2}` — the
        // subset chain classifies narrowing/widening both ways; disjoint
        // allowed-sets are incomparable.
        let space = ProposalSpace {
            points: vec![
                point(),
                PolicyPoint {
                    capability: "other-cap".into(),
                    ..point()
                },
            ],
        };
        let p_none = policy("none", vec![]);
        let p_one = allow_for("cap-1");
        let mut p_both = p_one.clone();
        p_both.rules.push(FlowRule {
            rule_id: "b".into(),
            selector: FlowSelector {
                domain: None,
                capability: Some("other-cap".into()),
                params: vec![],
            },
            condition: None,
            decision: FlowDecision::Allow,
            enforcement: crate::flow::EnforcementClass::Deterministic,
            remedies_hint: vec![],
        });
        assert_eq!(
            classify_policy_edit(&p_one, &p_both, &space),
            PolicyEditClass::Widening
        );
        assert_eq!(
            classify_policy_edit(&p_both, &p_one, &space),
            PolicyEditClass::Narrowing
        );
        assert_eq!(
            classify_policy_edit(&p_none, &p_one, &space),
            PolicyEditClass::Widening
        );
        assert_eq!(
            classify_policy_edit(&p_one, &p_one, &space),
            PolicyEditClass::Narrowing
        );
        // Disjoint allowed-sets — incomparable.
        let p_other = allow_for("other-cap");
        assert_eq!(
            classify_policy_edit(&p_one, &p_other, &space),
            PolicyEditClass::Incomparable
        );
    }

    #[test]
    fn apply_gates_by_origin() {
        let p_deny = policy(
            "old",
            vec![rule(
                "d",
                FlowDecision::Deny {
                    reason: "PolicyDenied".into(),
                },
            )],
        );
        let p_allow = policy("new", vec![rule("a", FlowDecision::Allow)]);
        let space = ProposalSpace {
            points: vec![point()],
        };
        // A delegate-class proposer may not widen.
        let model = Origin::model("m", "r", "x");
        assert!(matches!(
            apply_policy_edit(&p_deny, &p_allow, &model, &space),
            Err(PolicyEditError::WideningRequiresHuman { .. })
        ));
        // A delegate may narrow.
        assert_eq!(
            apply_policy_edit(&p_allow, &p_deny, &model, &space).unwrap(),
            PolicyEditClass::Narrowing
        );
        // A human may widen.
        let human = Origin::human("alice", HumanRole::Principal);
        assert_eq!(
            apply_policy_edit(&p_deny, &p_allow, &human, &space).unwrap(),
            PolicyEditClass::Widening
        );
        // An evolution origin is refused outright even for a widening it
        // would never get to apply.
        let evo = Origin::evolution("cand-1", "hyp-1");
        assert!(matches!(
            apply_policy_edit(&p_deny, &p_allow, &evo, &space),
            Err(PolicyEditError::EvolutionOrigin { .. })
        ));
    }
}
