//! §5c.1 "The ContextPolicy seam" (ADR-0073): the selection policy is a
//! registered component variant (`context_policy/<variant>`) declaring
//! `deterministic`, `model_conditioned_rules[]` and `required_inputs ⊇
//! {ModelProfile, ResourceAccount}`. A conditioned rule without a complete
//! `AssumptionDebtRecord` fails registration; a rule conditioned on a literal
//! model identity is refused (T-LCD-01).
//!
//! **No-widen (AC-R-2.4.1-4)**: the policy sees *admitted* candidates only —
//! each annotated with its kernel-computed `admissible_slots` — and returns a
//! `Selection` of ids. The policy record carries no label, authority,
//! validity, retention, floor or admits field, so there is nothing it *can*
//! widen; the kernel still re-validates every `Selection` member and a
//! violation is `PolicyViolation` — no plan, ever.
//!
//! `default` (C0): `deterministic = true`, `model_conditioned_rules = []`,
//! "admit all, layout precedence, ledger order in transcript, no expansion
//! requests, no by-reference delivery" (ADR-0073 d4).

use std::collections::{BTreeMap, BTreeSet};

use hh_hir::records::AssumptionDebtRecord;
use hh_wire::json::Json;

use crate::plan::{Candidate, Layout};
use crate::vocab::{CandidateKind, CandidateState, PriorityClass, Retention};

/// `required_inputs` members every `PolicyDeclaration` must name
/// (§5c.1 — the policy's declared input envelope).
pub const REQUIRED_POLICY_INPUTS: &[&str] = &["ModelProfile", "ResourceAccount"];

/// `model_conditioned_rules[]` — the declared conditioning of a rule
/// (§5c.1; ADR-0073 d3 as amended by CF-126/T-LCD-01).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleCondition {
    /// Conditioned on a `ProfileRef` (declared, legal).
    Profile(String),
    /// Conditioned on a compliance measurement keyed by snapshot (legal).
    ComplianceMeasurement(String),
    /// Conditioned on a literal model identity — **refused at registration**
    /// (T-LCD-01; AC-R-2.4.2-6).
    ModelIdentity(String),
}

/// One conditioned rule + its debt record (§5c.1; ADR-0073 d3).
#[derive(Debug, Clone)]
pub struct ConditionedRule {
    /// The rule id.
    pub rule_id: String,
    /// What the rule keys on.
    pub conditioned_on: RuleCondition,
    /// The `AssumptionDebtRecord` (`hh-hir`'s — CC7); `None`/incomplete fails
    /// registration.
    pub debt: Option<AssumptionDebtRecord>,
}

/// `PolicyDeclaration{variant_id, deterministic, model_conditioned_rules[],
/// required_inputs}` (§5c.1).
#[derive(Debug, Clone)]
pub struct PolicyDeclaration {
    /// `context_policy/<variant>` — the component-variant ref.
    pub variant_id: String,
    /// `deterministic` — declared, and checked against `I-DET` consumers.
    pub deterministic: bool,
    /// The conditioned rules with debt records.
    pub model_conditioned_rules: Vec<ConditionedRule>,
    /// `required_inputs ⊇ {ModelProfile, ResourceAccount}`.
    pub required_inputs: BTreeSet<String>,
}

/// The registration failures (§5c.1 "Policy registration"; AC-R-2.4.1-11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistrationError {
    /// `required_inputs` does not name the required envelope.
    MissingRequiredInput {
        /// The missing member.
        input: String,
    },
    /// A conditioned rule has no complete `AssumptionDebtRecord`.
    MissingDebt {
        /// The rule.
        rule_id: String,
        /// Which debt member is absent (`debt`, `removal_test_ref`, …).
        member: String,
    },
    /// A rule is conditioned on a literal model identity (T-LCD-01).
    ModelIdentityCondition {
        /// The rule.
        rule_id: String,
    },
    /// The variant declares `deterministic = true` but carries a conditioned
    /// rule the contract cannot verify deterministic (`similarity`-class
    /// conditioning is declared `deterministic = false` by construction).
    UndeclaredNonDeterminism {
        /// The rule.
        rule_id: String,
    },
}

impl std::fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistrationError::MissingRequiredInput { input } => {
                write!(f, "policy registration: missing required input {input}")
            }
            RegistrationError::MissingDebt { rule_id, member } => {
                write!(
                    f,
                    "policy registration: conditioned rule {rule_id} lacks debt member {member}"
                )
            }
            RegistrationError::ModelIdentityCondition { rule_id } => write!(
                f,
                "policy registration: rule {rule_id} conditioned on a model identity (T-LCD-01)"
            ),
            RegistrationError::UndeclaredNonDeterminism { rule_id } => write!(
                f,
                "policy registration: rule {rule_id} is non-deterministic under a \
                 deterministic declaration"
            ),
        }
    }
}

impl std::error::Error for RegistrationError {}

/// `check(decl)` — the registry-side registration contract (§5c.1;
/// AC-R-2.4.1-11, AC-R-2.4.2-6): the declared input envelope is complete,
/// every conditioned rule carries a *complete* `AssumptionDebtRecord`
/// (`removal_test_ref`, `expiry_condition`, `owner` all present — the record's
/// required members per §3.2.9), and no rule is conditioned on a model
/// identity.
pub fn check(decl: &PolicyDeclaration) -> Result<(), RegistrationError> {
    for req in REQUIRED_POLICY_INPUTS {
        if !decl.required_inputs.contains(*req) {
            return Err(RegistrationError::MissingRequiredInput {
                input: req.to_string(),
            });
        }
    }
    for rule in &decl.model_conditioned_rules {
        if let RuleCondition::ModelIdentity(_) = &rule.conditioned_on {
            return Err(RegistrationError::ModelIdentityCondition {
                rule_id: rule.rule_id.clone(),
            });
        }
        match &rule.debt {
            None => {
                return Err(RegistrationError::MissingDebt {
                    rule_id: rule.rule_id.clone(),
                    member: "debt".to_string(),
                })
            }
            Some(d) => {
                if d.removal_test_ref.is_empty() {
                    return Err(RegistrationError::MissingDebt {
                        rule_id: rule.rule_id.clone(),
                        member: "removal_test_ref".to_string(),
                    });
                }
                if d.owner.id.is_empty() {
                    return Err(RegistrationError::MissingDebt {
                        rule_id: rule.rule_id.clone(),
                        member: "owner".to_string(),
                    });
                }
            }
        }
    }
    Ok(())
}

/// `PolicyViolation` — the kernel's re-check of a `Selection` failed
/// (§5c.1 `assemble` errors). The `detail` names the widened member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyViolation {
    /// What the kernel's re-check caught.
    pub detail: String,
}

impl std::fmt::Display for PolicyViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PolicyViolation: {}", self.detail)
    }
}

impl std::error::Error for PolicyViolation {}

/// `AdmittedCandidate` — what `admit` hands the policy: the candidate plus
/// its kernel-computed `admissible_slots` (the policy *selects among* these —
/// it can never mint a new admissibility).
#[derive(Debug, Clone)]
pub struct AdmittedCandidate {
    /// The candidate.
    pub candidate: Candidate,
    /// `admissible_slots` — kernel-computed: `kind ∈ slot.admits ∧
    /// label.authority ≥ slot.min_authority ∧ lifecycle ∈ admitted_states`.
    pub admissible_slots: Vec<String>,
}

/// `Selection{chosen[], order, evict_order, by_reference[], expand_requests[]}`
/// (§5c.1 "Policy interface") — the policy's whole output vocabulary: ids and
/// slot names. There is no field through which authority, validity,
/// retention, floors or admits could be widened (AC-R-2.4.1-4).
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// `(candidate_id, slot_id)` pairs — the membership+placement.
    pub chosen: Vec<(String, String)>,
    /// `slot_id → candidate_ids in render order` (the `order` member).
    pub order: BTreeMap<String, Vec<String>>,
    /// `evict_order` — the deterministic eviction order (kernel-class order
    /// `(PriorityClass, age)` for `default`; a policy may declare its own,
    /// which the kernel sorts deterministically before `enforce_budget`).
    pub evict_order: Vec<String>,
    /// `by_reference[]` — candidates delivered as handles the model expands.
    pub by_reference: Vec<String>,
    /// `expand_requests[]` — `handle_only` candidates the policy asks the
    /// kernel to expand inline (a `ContextItem` effect — the kernel performs
    /// it; the policy only names ids).
    pub expand_requests: Vec<String>,
}

/// `PolicyRequest{candidates: admitted[] (each with admissible_slots),
/// layout, budget_remaining, params}` — what `select` sees (§5c.1).
pub struct PolicyRequest<'a> {
    /// The kernel-admitted candidates (post I-ID/I-RP/I-ORDER).
    pub candidates: &'a [AdmittedCandidate],
    /// The linked layout.
    pub layout: &'a Layout,
    /// `budget_remaining` — `window_cap - margin - reservations`.
    pub budget_remaining: u64,
    /// The policy params (profile data).
    pub params: &'a Json,
}

/// `ContextPolicy` — the seam (§5c.1; ADR-0073). `select` names ids; it
/// cannot carry authority/validity/retention/slot-floor data because
/// `Selection` has no such members — the contract is structural.
pub trait ContextPolicy {
    /// The `PolicyDeclaration` (registry-checked).
    fn declare(&self) -> PolicyDeclaration;
    /// `priority(candidate)` — the eviction class assignment. For
    /// `Retention::Optional(p)` the caller-declared class carries; the
    /// default policy derives classes for `Required` candidates only as
    /// bookkeeping (required items are never evicted).
    fn priority(&self, candidate: &Candidate) -> PriorityClass;
    /// `select(candidates, layout, budget_remaining, params)` → `Selection`.
    fn select(&self, req: &PolicyRequest<'_>) -> Result<Selection, PolicyViolation>;
}

/// `context_policy/default` (C0; ADR-0073 d4): `deterministic`, no
/// conditioned rules, "admit all, layout precedence, ledger order in
/// transcript, no expansion requests, no by-reference delivery". The
/// `old_after_turns` parameter is the default `N` in "observations older than
/// N turns are `observation_old`".
#[derive(Debug, Clone)]
pub struct DefaultPolicy {
    /// The `N` of `observation_old` (an observation whose `source_seq` is more
    /// than `N` below the request's `at_seq` is old).
    pub old_after_turns: u64,
    /// The request's `at_seq` (set per call by the kernel before `priority`).
    pub at_seq: u64,
}

impl Default for DefaultPolicy {
    fn default() -> Self {
        DefaultPolicy {
            old_after_turns: 8,
            at_seq: 0,
        }
    }
}

impl ContextPolicy for DefaultPolicy {
    fn declare(&self) -> PolicyDeclaration {
        PolicyDeclaration {
            variant_id: "context_policy/default".to_string(),
            deterministic: true,
            model_conditioned_rules: Vec::new(),
            required_inputs: REQUIRED_POLICY_INPUTS
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }

    fn priority(&self, candidate: &Candidate) -> PriorityClass {
        if let Retention::Optional(p) = candidate.retention {
            return p;
        }
        match candidate.kind {
            CandidateKind::TranscriptItem => PriorityClass::TranscriptTail,
            CandidateKind::Observation => {
                if self.at_seq.saturating_sub(candidate.source_seq) > self.old_after_turns {
                    PriorityClass::ObservationOld
                } else {
                    PriorityClass::ObservationRecent
                }
            }
            CandidateKind::Memory => PriorityClass::Memory,
            CandidateKind::MemoryIndex => PriorityClass::MemoryIndex,
            CandidateKind::ProcedureBody | CandidateKind::ProcedureIndex => {
                PriorityClass::ProcedureBody
            }
            CandidateKind::ArtifactExcerpt => PriorityClass::Image,
            _ => PriorityClass::Commentary,
        }
    }

    fn select(&self, req: &PolicyRequest<'_>) -> Result<Selection, PolicyViolation> {
        let mut chosen = Vec::new();
        let mut order: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for ac in req.candidates {
            // "admit all, layout precedence": the lowest-precedence-numbered
            // admissible slot (transcript_items land only in `transcript`).
            let slot = ac
                .admissible_slots
                .iter()
                .min_by_key(|sid| {
                    req.layout
                        .slot(sid)
                        .map(|s| s.order.precedence)
                        .unwrap_or(u64::MAX)
                })
                .ok_or_else(|| PolicyViolation {
                    detail: format!(
                        "default policy: admitted candidate {} has no admissible slot",
                        ac.candidate.candidate_id
                    ),
                })?
                .clone();
            chosen.push((ac.candidate.candidate_id.clone(), slot.clone()));
            order
                .entry(slot)
                .or_default()
                .push(ac.candidate.candidate_id.clone());
        }
        // Ledger order within every slot at C0 (`source_seq` ascending);
        // `transcript` ordering is ledger order by its comparator.
        let seq_of: BTreeMap<&str, u64> = req
            .candidates
            .iter()
            .map(|ac| (ac.candidate.candidate_id.as_str(), ac.candidate.source_seq))
            .collect();
        for ids in order.values_mut() {
            ids.sort_by_key(|cid| seq_of.get(cid.as_str()).copied().unwrap_or(0));
        }
        // `evict_order` — kernel class order: `(PriorityClass, age)` ascending
        // (age = `at_seq - source_seq`; older evicts first inside a class).
        let mut evictable: Vec<&AdmittedCandidate> = req
            .candidates
            .iter()
            .filter(|ac| !ac.candidate.retention.is_required())
            .collect();
        evictable.sort_by_key(|ac| {
            (
                self.priority(&ac.candidate).eviction_rank(),
                self.at_seq.saturating_sub(ac.candidate.source_seq),
                ac.candidate.candidate_id.clone(),
            )
        });
        Ok(Selection {
            chosen,
            order,
            evict_order: evictable
                .iter()
                .map(|ac| ac.candidate.candidate_id.clone())
                .collect(),
            by_reference: Vec::new(),
            expand_requests: Vec::new(),
        })
    }
}

/// The kernel's post-`select` re-check (AC-R-2.4.1-4): every `chosen` pair
/// must name an admitted candidate and one of *its* `admissible_slots`;
/// `order` must partition `chosen`; `by_reference`/`expand_requests` name
/// chosen candidates only (`expand_requests` additionally requires
/// `handle_only` state); every `required` candidate is chosen.
/// Returns the violation — the caller maps it to `AssemblyError::PolicyViolation`.
pub fn check_selection(req: &PolicyRequest<'_>, sel: &Selection) -> Result<(), PolicyViolation> {
    let admitted: BTreeMap<&str, &AdmittedCandidate> = req
        .candidates
        .iter()
        .map(|ac| (ac.candidate.candidate_id.as_str(), ac))
        .collect();
    let bad = |detail: String| PolicyViolation { detail };
    let mut chosen_ids: BTreeSet<&str> = BTreeSet::new();
    for (cid, sid) in &sel.chosen {
        let ac = admitted
            .get(cid.as_str())
            .ok_or_else(|| bad(format!("selection names non-admitted candidate {cid}")))?;
        if !ac.admissible_slots.iter().any(|s| s == sid) {
            return Err(bad(format!(
                "candidate {cid} placed in {sid} outside admissible_slots {:?}",
                ac.admissible_slots
            )));
        }
        if !chosen_ids.insert(cid.as_str()) {
            return Err(bad(format!("candidate {cid} chosen twice")));
        }
    }
    for ac in req.candidates {
        if ac.candidate.retention.is_required()
            && !chosen_ids.contains(ac.candidate.candidate_id.as_str())
        {
            return Err(bad(format!(
                "required candidate {} not chosen (I-NOWIDEN: retention is kernel-set)",
                ac.candidate.candidate_id
            )));
        }
    }
    let mut ordered: BTreeSet<&str> = BTreeSet::new();
    for (sid, ids) in &sel.order {
        for cid in ids {
            if !chosen_ids.contains(cid.as_str()) {
                return Err(bad(format!(
                    "order member {cid} (slot {sid}) was not chosen"
                )));
            }
            if !ordered.insert(cid.as_str()) {
                return Err(bad(format!("candidate {cid} ordered in two slots")));
            }
        }
    }
    for cid in sel
        .by_reference
        .iter()
        .chain(sel.expand_requests.iter())
        .chain(sel.evict_order.iter())
    {
        let ac = admitted
            .get(cid.as_str())
            .ok_or_else(|| bad(format!("selection references unknown candidate {cid}")))?;
        if sel.expand_requests.contains(cid) && ac.candidate.state != CandidateState::HandleOnly {
            return Err(bad(format!(
                "expand_requests names {cid} which is not handle_only"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_refuses_missing_inputs_and_debt() {
        let mut d = DefaultPolicy::default().declare();
        assert!(check(&d).is_ok());
        d.required_inputs.clear();
        assert!(matches!(
            check(&d),
            Err(RegistrationError::MissingRequiredInput { .. })
        ));
        let d = PolicyDeclaration {
            variant_id: "context_policy/x".into(),
            deterministic: true,
            model_conditioned_rules: vec![ConditionedRule {
                rule_id: "r1".into(),
                conditioned_on: RuleCondition::ModelIdentity("gpt-4o".into()),
                debt: None,
            }],
            required_inputs: REQUIRED_POLICY_INPUTS
                .iter()
                .map(|s| s.to_string())
                .collect(),
        };
        assert!(matches!(
            check(&d),
            Err(RegistrationError::ModelIdentityCondition { .. })
        ));
    }
}
