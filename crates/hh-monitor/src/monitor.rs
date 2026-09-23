//! `authorize` — the monitor's decision function (§5g.1 §2.1/§2.2; ADR-0052
//! D1–D3). Steps **0–3 and 6** landed at Stage 1; step 4 is its authority-form
//! half (the Π gate + the ADR-0031 floor + Π-12's unattended transform, all
//! inside [`PolicyTable::evaluate`]/[`floor_verdict`]); **step 5** (the
//! persistence ceiling — check 6, ADR-0035 D3 + the `store` dimension) and
//! **steps 7–8** (approval feasibility + the lease cache) land at Stage 2.
//! The hook/auto_reviewer/human stages of an `ask` are the §5g.7 §4
//! escalation chain the dispatcher drives over [`approval::run_chain`] —
//! `authorize` is pure and never runs a hook.
//!
//! Short-circuit on first deny (§2.2). The decision is a pure function of the
//! recorded inputs — [`Proposal`] carries `AssessmentInputs` as data (the
//! executor-registered assessors' *recorded* results; the monitor never reads
//! tool text, ADR-0100).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_compiler::equiv::SurfaceBinding;
use hh_compiler::plan::PinnedRef;
use hh_hir::kinds::EffectClass;
use hh_hir::kinds::EffectDomain;
use hh_hir::records::ToolCapabilityRecord;
use hh_ontology::risk::{RiskClass, RiskReversibility, RiskScope};
use hh_provenance::flow::{self, FlowContract, FlowInput, FlowVerdict};
use hh_provenance::{AuthorityClass, Label, ProvenanceRecord, TaintTag};
use hh_wire::json::Json;

use crate::approval::{self, ApprovalLease, ApprovalMode, ApprovalState, EscalationInput};
use crate::args::{self, ArgError, CanonicalArgs};
use crate::assess::{self, AssessmentInputs, MemoryScope};
use crate::decision::{CheckRecord, Decider, Decision, DecisionScope, DenyReason, KernelDecision};
use crate::handle::{HandleId, OriginBasis};
use crate::policy::{Mode, PiContext, PiVerdict, PolicyTable};
use crate::table::HandleTable;
use hh_provenance::PersistenceScope;

/// `Proposal` — the `action.effect.intended` payload plus `args_provenance`
/// and `self_report?` (§3). Every member is a canonical record — no `Text`,
/// ever (I-H1; AC-R-2.8.1-11).
#[derive(Debug, Clone)]
pub struct Proposal {
    /// The effect's identity coordinate.
    pub effect_id: String,
    /// The attempt cycle (the gate key's second member).
    pub attempt_no: u64,
    /// The proposing `AgentProcess` (`semantic_id`).
    pub proposer: String,
    /// The invoked capability (pinned).
    pub capability_ref: PinnedRef,
    /// The declared effect class (`EffectClass(p)`).
    pub effect: EffectClass,
    /// The **surface** arguments — `eval` projects them to canonical
    /// parameters (I-H5).
    pub surface_args: Json,
    /// The arguments' provenance (`taint(args)` at step 1) — mandatory
    /// (ADR-0035 D4); `None` ⇒ `MissingProvenance`.
    pub args_provenance: Option<ProvenanceRecord>,
    /// The context label the kernel stamped for this call (P3 — I-H9: the
    /// kernel computes it, the proposal carries the stamped record).
    pub context_label: Label,
    /// The model's risk self-report — raise-only (step 3 (iv)).
    pub self_report: Option<RiskClass>,
    /// The recorded assessor inputs (the `assessment_inputs` the deterministic
    /// assessors computed at `resolve` — recorded, never re-read).
    pub inputs: AssessmentInputs,
    /// For `spawn_process`/`Delegate` proposals: the requested child grants
    /// (step 6's operands — checked against the covering handle).
    pub requested_grants: Vec<hh_hir::records::Grant>,
    /// The recorded containment-floor verdict (§5g.4; ADR-0062 D3 — the
    /// `authorize` precondition). The kernel computes it through
    /// `hh_containment` (`admit::floor_gate`) at resolve and stamps it on
    /// the proposal like `inputs` — the monitor never evaluates
    /// containment itself; the operation edge is R-2.8.4 → R-2.8.1.
    pub containment: ContainmentGate,
    /// The C2 flow inputs (§5g.2; R-2.8.2) — the per-parameter labels the
    /// kernel stamped at dispatch, the shape-endorsement flags D-ROBUST
    /// reads, the committed-effect projection `committed`/`every_committed`
    /// atoms consume, and the recorded detector verdicts. Empty = the flow
    /// stage still runs on a declared `flow_contract` (its checks degrade
    /// to `EvaluationError` where a required label is unrecorded).
    pub flow: FlowInputs,
    /// The decision's logical time (`at` — the seq the `time` constraint's
    /// bound compares against).
    pub at: u64,
}

/// `FlowInputs` — the flow-plane members `authorize` consumes (§5g.2 §3's
/// `Proposal` extension). Every member is a kernel-stamped canonical record
/// (I-H1): `param_labels` is keyed by the **canonical** (post-`SurfaceArgMap`)
/// parameter name — a handle argument's entry carries the handle's label.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlowInputs {
    /// Canonical parameter → its `Label` (`L(args)`'s per-parameter form).
    pub param_labels: BTreeMap<String, Label>,
    /// Parameters whose value carries a `validator` shape endorsement —
    /// D-ROBUST's exemption set (I-F2).
    pub shape_endorsed: BTreeSet<String>,
    /// `project(run, effects, until_seq)` — the committed-effect projection
    /// the `committed`/`every_committed`/`count` atoms read.
    pub committed: Vec<hh_provenance::flow::CommittedEffect>,
    /// Recorded deterministic detector verdicts — `"<validator_ref>:<param>"`.
    /// A `detector` atom on an unrecorded key is an `EvaluationError`, never
    /// `false`.
    pub detectors: BTreeMap<String, bool>,
}

/// `ContainmentGate` — the recorded containment-floor verdict `authorize`
/// consumes (ADR-0062 D3; R-2.8.4). A recorded input like
/// `AssessmentInputs` — produced by `hh_containment` from the effective
/// policy + the attach `ContainmentReport` + the proposal's canonical
/// arguments, never computed inside the monitor.
#[derive(Debug, Clone, PartialEq)]
pub enum ContainmentGate {
    /// Required field-group evidence is in force and `admits()` admitted
    /// (or the domain engages no governed surface).
    Clear,
    /// `admits()` returned `refused` or `amendable` — an affirmative
    /// boundary denial: `deny{containment}` before any check; Π is never
    /// consulted and `ask` is never produced (AC-R-2.8.4-14). `detail` is
    /// the recorded tag (`refused:<reason>` / `amendable:<diff>`).
    Denied {
        /// The recorded admits tag.
        detail: String,
    },
    /// A required field group's `enforcement_evidence` is `unknown` (or no
    /// report exists / the stored report is stale) —
    /// `deny{ContainmentUnverified}` before any check (I-C4 fail-closed).
    /// `group` spells the field group, `attach` (helper absent / backend
    /// unsupported) or `report` (stale on resume).
    Unverified {
        /// The unverified subject's spelling.
        group: String,
    },
}

/// The operational (non-decision) failure of `authorize` — distinct from a
/// `deny` *decision*: these mean the proposal couldn't be evaluated at all.
#[derive(Debug, Clone, PartialEq)]
pub enum MonitorError {
    /// `capability_ref` names no registered capability (the `resolve`-stage
    /// refusal `CapabilityUnresolvable`).
    CapabilityUnresolvable {
        /// The unresolved semantic id.
        capability: String,
    },
    /// The capability carries no `SurfaceBinding` (an unbound surface can't
    /// produce canonical params).
    SurfaceUnbound {
        /// The capability semantic id.
        capability: String,
    },
}

/// A capability registration — the pinned `ToolCapability` record plus its
/// compiled surface binding (the `SurfaceArgMap` the evaluator reads).
#[derive(Debug, Clone)]
pub struct CapabilityEntry {
    /// The capability record (semantic).
    pub record: ToolCapabilityRecord,
    /// The compiled surface binding (ADR-0090).
    pub binding: SurfaceBinding,
}

/// `Monitor` — the Stage-1 reference monitor: the handle table, Π, the
/// registered proposers (`semantic_id → label`) and capabilities, the run's
/// live coordinates and the decision-open flag the guard trips on.
#[derive(Debug)]
pub struct Monitor {
    /// The kernel handle table (derived view over `security.permission.*`).
    pub table: HandleTable,
    /// The Π table in force (`version_id` lands in `policy_ref`).
    pub policy: PolicyTable,
    /// Registered proposers — `AgentProcess.semantic_id → label` (the
    /// proposer's authority for `min(authority(proposer), ctx)`).
    pub proposers: BTreeMap<String, Label>,
    /// Registered capabilities — `semantic_id → CapabilityEntry`.
    pub capabilities: BTreeMap<String, CapabilityEntry>,
    /// The current effect scope's identity (liveness for `effect`-expiry).
    pub effect_id: String,
    /// The current turn id.
    pub turn_id: String,
    /// The run id.
    pub run_id: String,
    /// The session ref (empty when none).
    pub session: String,
    /// Whether a decision is currently open (the `DecisionGuard` flag —
    /// `ModelCallDuringDecision` trips on a model call while set).
    pub decision_open: bool,
    /// The approval fold's read side (steps 7–8): the live leases and the
    /// The sealed `HarnessRule{auto_review}` set the reviewer chain's
    /// `auto_reviewer` stage runs (`auto_review_rules(sealed)` extracted at
    /// monitor setup — I-P2's endorser). Empty = no rule can match.
    pub auto_review_rules: Vec<crate::approval::AutoReviewRule>,
    /// The sealed repeated-denial policy (`None` = no ceiling — a denial
    /// never auto-fires a fallback).
    pub denial_policy: Option<crate::approval::DenialPolicy>,
    /// `approvals.requested` count. The dispatcher maintains it from the
    /// `security.permission.*` trail — a derived view, never a second truth
    /// (CC1).
    pub approvals: ApprovalState,
    /// The `approvals.requested` hard ceiling (ADR-0040 — `None` = the
    /// dimension is unbudgeted). Exhaustion converts the next
    /// *human-targeted* ask to `deny{ApprovalsExhausted}` — a serving lease
    /// resolves regardless (§5g.7's exhaustion row).
    pub approvals_max: Option<u64>,
    /// The run's declared approval mode (I-P4 — `async` defers the human
    /// stage; the request's `mode` member carries it to the surface).
    pub approval_mode: ApprovalMode,
    /// The lease key's policy leg — `policy_fingerprint(version_id, mode,
    /// narrowing_leaf_ids)` computed at monitor setup; a Π/mode change is a
    /// new fingerprint and revokes every old lease by key construction
    /// (ADR-0071 D1).
    pub policy_fingerprint: String,
}

impl Monitor {
    /// A monitor over the given table and policy (empty registries — tests and
    /// the run-start path populate them).
    pub fn new(table: HandleTable, policy: PolicyTable) -> Monitor {
        let policy_fingerprint = approval::policy_fingerprint(
            &policy.version_id,
            match policy.mode {
                Mode::Attended => "attended",
                Mode::Unattended => "unattended",
            },
            &[],
        );
        Monitor {
            table,
            policy,
            proposers: BTreeMap::new(),
            capabilities: BTreeMap::new(),
            effect_id: String::new(),
            turn_id: String::new(),
            run_id: String::new(),
            session: String::new(),
            decision_open: false,
            auto_review_rules: Vec::new(),
            denial_policy: None,
            approvals: ApprovalState::default(),
            approvals_max: None,
            approval_mode: ApprovalMode::Sync,
            policy_fingerprint,
        }
    }

    /// `escalation_input(p, canonical, cap, eff, risk)` — the record the
    /// escalation chain consumes (§5g.7 §4; the `EscalationInput` data-model
    /// row): the lease legs (exact args hash + each declared `ActionPattern`
    /// projection), the never-auto members, the budget state and the
    /// fingerprint. Pure over canonical records — the caller (dispatch)
    /// passes it to [`approval::run_chain`] for the hook/auto_reviewer/human
    /// stages.
    pub fn escalation_input(
        &self,
        p: &Proposal,
        canonical: &CanonicalArgs,
        cap: &CapabilityEntry,
        eff: AuthorityClass,
        risk: RiskClass,
    ) -> EscalationInput {
        let args_str: BTreeMap<String, String> = canonical
            .params
            .iter()
            .map(|(k, v)| (k.clone(), v.to_canonical_string()))
            .collect();
        let pattern_keys = cap
            .record
            .action_patterns
            .iter()
            .map(|d| {
                approval::ActionPattern {
                    fields: d.fields.iter().cloned().collect(),
                }
                .key_material(&args_str)
            })
            .collect();
        let irreversible = risk.reversibility == RiskReversibility::Irreversible;
        EscalationInput {
            effect_id: p.effect_id.clone(),
            scope_ref: self.run_id.clone(),
            capability_ref: p.capability_ref.clone(),
            args_canonical_hash: args::canonical_args_hash(canonical),
            pattern_keys,
            domain: p.effect.domain,
            risk,
            eff,
            holder: p.proposer.clone(),
            irreversible,
            mode: self.policy.mode,
            unattended_policy: self.policy.unattended_policy,
            approval_mode: self.approval_mode,
            policy_fingerprint: self.policy_fingerprint.clone(),
            approvals_used: self.approvals.stats.requested,
            approvals_max: self.approvals_max,
            // The sealed-definition `ask` leaf and the revoked/stale legs are
            // the dispatcher's scan (it reads the sealed rules and the live
            // grant table); the consent-step member reads the class.
            explicit_ask: false,
            consent_step: p.effect.domain == EffectDomain::MessageHuman,
            revoked_or_stale: false,
            user_scope_persistence: self.write_scope(p, canonical) == Some(PersistenceScope::User),
            // `irreversible` is never batched; `permission_request` effects
            // are their own owed-decision records (never coalesced into a
            // batch).
            batchable: !irreversible && p.effect.domain != EffectDomain::PermissionRequest,
            context_authority: p.context_label.authority,
        }
    }

    /// The lease serving `input` — the step-8 lookup over the exact-args leg
    /// plus each declared pattern leg (the dispatcher's `run_chain` lease
    /// stage computes the same keys — one rule, one spelling).
    pub fn lease_hit(&self, input: &EscalationInput) -> Option<(String, ApprovalLease)> {
        for material in approval::lease_candidates(input) {
            let key = approval::lease_key(
                &input.capability_ref,
                &material,
                approval::LeaseScope::Run,
                &input.policy_fingerprint,
            );
            if let Some(l) = self.approvals.leases.get(&key) {
                if approval::lease_serves(l, input) {
                    return Some((key, l.clone()));
                }
            }
        }
        None
    }

    /// Whether a live `policy_rule`-basis handle held by `proposer` covers
    /// `domain` over `canonical` — the `pre_authorized` assessment input's
    /// derivation (ADR-0053 D5): the sealed `pre_authorize` rule's *handle
    /// record* confers, never a flag or a name (I-H1). `Tri::Unknown` only
    /// when the input itself cannot be decided — a missing handle is `No`.
    pub fn pre_authorized(
        &self,
        proposer: &str,
        domain: EffectDomain,
        canonical: &CanonicalArgs,
    ) -> crate::assess::Tri {
        let covered = self.table.handles.values().any(|h| {
            h.origin_basis == OriginBasis::PolicyRule
                && h.holder.semantic_id == proposer
                && h.is_live(&self.effect_id, &self.turn_id, &self.run_id, &self.session)
                && h.grants.iter().any(|g| {
                    g.effect.domain == domain && args::grant_scope_covers(&g.scope, canonical)
                })
        });
        if covered {
            crate::assess::Tri::Yes
        } else {
            crate::assess::Tri::No
        }
    }

    /// The write's target persistence scope — `memory_write` reads the
    /// recorded `memory_scope` input; `fs_write` reads a `scope`/
    /// `persistence_scope` canonical param (or any scoped value spelling a
    /// scope). `None` = no persistence-scope write — the ceiling is exempt.
    fn write_scope(&self, p: &Proposal, canonical: &CanonicalArgs) -> Option<PersistenceScope> {
        match p.effect.domain {
            EffectDomain::MemoryWrite => p.inputs.memory_scope.map(|m| match m {
                MemoryScope::Run => PersistenceScope::Run,
                MemoryScope::Session => PersistenceScope::Session,
                MemoryScope::Project => PersistenceScope::Project,
                MemoryScope::User => PersistenceScope::User,
            }),
            EffectDomain::FsWrite => canonical
                .params
                .get("scope")
                .or_else(|| canonical.params.get("persistence_scope"))
                .and_then(Json::as_str)
                .and_then(parse_persistence_scope)
                .or_else(|| {
                    canonical
                        .scoped
                        .values()
                        .filter_map(|v| v.as_str())
                        .find_map(parse_persistence_scope)
                }),
            _ => None,
        }
    }

    /// Whether a live `approval`-basis handle held by `proposer` grants
    /// `domain` — the `+ approval` half of the `user`-scope write ceiling
    /// (the approval *record* confers, never a name).
    fn approval_evidence(&self, proposer: &str, domain: EffectDomain) -> bool {
        self.table.handles.values().any(|h| {
            h.origin_basis == OriginBasis::Approval
                && h.holder.semantic_id == proposer
                && h.is_live(&self.effect_id, &self.turn_id, &self.run_id, &self.session)
                && h.grants.iter().any(|g| g.effect.domain == domain)
        })
    }

    /// Step 5 — the persistence ceiling (check 6; ADR-0035 D3 as amended by
    /// the `store` dimension, ADR-0080 D2/CF-172): a `memory_write`/`fs_write`
    /// targeting a scope above its write ceiling denies
    /// `ScopeCeilingExceeded`. `run`/`turn` exempt; `session` needs
    /// `delegate`; `project` needs `principal`; `user` needs `principal` plus
    /// approval evidence; `definition` is seal-only — a proposal never
    /// reaches it. `store = memory` relaxes `project`/`user` to `delegate`
    /// (the memory-store row).
    fn persistence_ceiling_violation(
        &self,
        p: &Proposal,
        canonical: &CanonicalArgs,
        eff: AuthorityClass,
    ) -> Option<String> {
        let scope = self.write_scope(p, canonical)?;
        let store_memory = p.effect.domain == EffectDomain::MemoryWrite
            && canonical
                .params
                .get("store")
                .and_then(Json::as_str)
                .map(|s| s == "memory")
                .unwrap_or(false);
        let exceeds = |min_eff: AuthorityClass, approval: bool| -> Option<String> {
            if eff < min_eff {
                return Some(format!("{}:eff<{}", scope.as_str(), min_eff.as_str()));
            }
            if approval && !self.approval_evidence(&p.proposer, p.effect.domain) {
                return Some(format!("{}:no_approval_evidence", scope.as_str()));
            }
            None
        };
        match scope {
            PersistenceScope::Turn | PersistenceScope::Run => None,
            PersistenceScope::Session => exceeds(AuthorityClass::Delegate, false),
            PersistenceScope::Project => {
                if store_memory {
                    exceeds(AuthorityClass::Delegate, false)
                } else {
                    exceeds(AuthorityClass::Principal, false)
                }
            }
            PersistenceScope::User => {
                if store_memory {
                    exceeds(AuthorityClass::Delegate, false)
                } else {
                    exceeds(AuthorityClass::Principal, true)
                }
            }
            PersistenceScope::Definition => Some("definition:seal_only".to_string()),
        }
    }

    /// `replay(p, recorded)` — the decision-replay half of the audit
    /// contract (§5g.1's replay row): re-derive the decision over the
    /// recorded inputs and compare member-wise. A mismatch is reported, never
    /// re-decided silently — `mismatches` names the differing members.
    pub fn replay(
        &self,
        p: &Proposal,
        recorded: &KernelDecision,
    ) -> Result<ReplayOutcome, MonitorError> {
        let fresh = self.authorize(p)?;
        let mut mismatches: Vec<String> = Vec::new();
        if fresh.decision != recorded.decision {
            mismatches.push("decision".to_string());
        }
        if fresh.effective_authority != recorded.effective_authority {
            mismatches.push("effective_authority".to_string());
        }
        if fresh.taint != recorded.taint {
            mismatches.push("taint".to_string());
        }
        if fresh.effective_risk_class != recorded.effective_risk_class {
            mismatches.push("effective_risk_class".to_string());
        }
        if fresh.handle_ids != recorded.handle_ids {
            mismatches.push("handle_ids".to_string());
        }
        if fresh.policy_ref != recorded.policy_ref {
            mismatches.push("policy_ref".to_string());
        }
        if fresh.checks != recorded.checks {
            mismatches.push("checks".to_string());
        }
        if fresh.decider != recorded.decider {
            mismatches.push("decider".to_string());
        }
        if fresh.decision_scope != recorded.decision_scope {
            mismatches.push("decision_scope".to_string());
        }
        Ok(ReplayOutcome {
            matched: mismatches.is_empty(),
            mismatches,
        })
    }

    /// `resolve_handles(holder, capability, canonical_args)` — the covering
    /// handles by domain + scope over canonical parameters (§2.1; I-H5).
    /// Scope-bearing parameters already resolved by [`args::eval`].
    pub fn resolve_handles(
        &self,
        holder: &str,
        domain: EffectDomain,
        canonical: &CanonicalArgs,
    ) -> Vec<(HandleId, hh_hir::records::Grant)> {
        self.table
            .covering_domain(
                holder,
                domain,
                &self.effect_id,
                &self.turn_id,
                &self.run_id,
                &self.session,
            )
            .into_iter()
            .filter(|(_, _, g)| args::grant_scope_covers(&g.scope, canonical))
            .map(|(id, _, g)| (id.clone(), g.clone()))
            .collect()
    }

    /// `authorize(p) → KernelDecision` — steps 0–3 + 6 (§2.2). The decision
    /// record is the `security.permission.decided` payload
    /// ([`crate::events::decided_payload`]); the ledger's complete-mediation
    /// gate makes it the precondition of `committed`.
    ///
    /// `decision_open` is held for the call's duration — a model call mid-
    /// decision trips `ModelCallDuringDecision` (the runtime half of
    /// AC-R-2.8.1-11). The flag is the caller's to inspect via
    /// [`crate::guard::DecisionGuard`]; `authorize` itself is pure.
    pub fn authorize(&self, p: &Proposal) -> Result<KernelDecision, MonitorError> {
        let mut checks: Vec<CheckRecord> = Vec::new();
        let deny = |checks: Vec<CheckRecord>,
                    step: u8,
                    reason: DenyReason,
                    eff: AuthorityClass,
                    taint: BTreeSet<TaintTag>,
                    risk: RiskClass| KernelDecision {
            effect_id: p.effect_id.clone(),
            decision: Decision::Deny {
                reason,
                remedies: Vec::new(),
            },
            effective_authority: eff,
            taint,
            effective_risk_class: risk,
            handle_ids: Vec::new(),
            policy_ref: self.policy.version_id.clone(),
            checks: {
                let mut c = checks;
                c.push(CheckRecord {
                    enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                    step,
                    outcome: "fail",
                    detail: reason.as_str().to_string(),
                });
                c
            },
            decider: Decider::Policy,
            decision_scope: DecisionScope::Once,
            cache_key: None,
            origin_permission_id: None,
            remedy_taken: None,
            assessment_inputs_ref: Some(assessment_inputs_ref(&p.inputs)),
        };
        let deny_rem = |mut checks: Vec<CheckRecord>,
                        step: u8,
                        reason: DenyReason,
                        remedies: Vec<flow::Remedy>,
                        eff: AuthorityClass,
                        taint: BTreeSet<TaintTag>,
                        risk: RiskClass| {
            checks.push(CheckRecord {
                step,
                outcome: "fail",
                detail: reason.as_str().to_string(),
                enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
            });
            KernelDecision {
                effect_id: p.effect_id.clone(),
                decision: Decision::Deny { reason, remedies },
                effective_authority: eff,
                taint,
                effective_risk_class: risk,
                handle_ids: Vec::new(),
                policy_ref: self.policy.version_id.clone(),
                checks,
                decider: Decider::Policy,
                decision_scope: DecisionScope::Once,
                cache_key: None,
                origin_permission_id: None,
                assessment_inputs_ref: Some(assessment_inputs_ref(&p.inputs)),
                remedy_taken: None,
            }
        };
        // The containment precondition (ADR-0062 D3; R-2.8.4) — BEFORE any
        // check: a required field group's `unknown` evidence ⇒
        // `deny{ContainmentUnverified}`; a recorded floor denial ⇒
        // `deny{containment}` — Π is never consulted and `ask` never
        // produced (AC-R-2.8.4-14). The precondition records at step 0 —
        // on a refusal the trail's first record is it.
        match &p.containment {
            ContainmentGate::Unverified { group } => {
                checks.push(CheckRecord {
                    enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                    step: 0,
                    outcome: "fail",
                    detail: format!("containment:unverified:{group}"),
                });
                return Ok(deny(
                    checks,
                    0,
                    DenyReason::ContainmentUnverified,
                    AuthorityClass::Unverified,
                    BTreeSet::new(),
                    RiskClass::UNKNOWN,
                ));
            }
            ContainmentGate::Denied { detail } => {
                checks.push(CheckRecord {
                    enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                    step: 0,
                    outcome: "fail",
                    detail: detail.clone(),
                });
                return Ok(deny(
                    checks,
                    0,
                    DenyReason::Containment,
                    AuthorityClass::Unverified,
                    BTreeSet::new(),
                    RiskClass::UNKNOWN,
                ));
            }
            ContainmentGate::Clear => {}
        }

        // Step 0 — well-formedness: provenance present, proposer registered,
        // every argument through `SurfaceArgMap`.
        let Some(args_prov) = &p.args_provenance else {
            return Ok(deny(
                checks,
                0,
                DenyReason::MissingProvenance,
                AuthorityClass::Unverified,
                BTreeSet::new(),
                RiskClass::UNKNOWN,
            ));
        };
        let Some(proposer_label) = self.proposers.get(&p.proposer) else {
            return Ok(deny(
                checks,
                0,
                DenyReason::UnknownProposer,
                AuthorityClass::Unverified,
                BTreeSet::new(),
                RiskClass::UNKNOWN,
            ));
        };
        let cap = self
            .capabilities
            .get(&p.capability_ref.semantic_id)
            .ok_or_else(|| MonitorError::CapabilityUnresolvable {
                capability: p.capability_ref.semantic_id.clone(),
            })?;
        let canonical = match args::eval(&cap.binding, &cap.record.scope_bindings, &p.surface_args)
        {
            Ok(c) => c,
            Err(ArgError::UnmappedArgument { .. }) => {
                return Ok(deny(
                    checks,
                    0,
                    DenyReason::UnmappedArgument,
                    proposer_label.authority,
                    BTreeSet::new(),
                    RiskClass::UNKNOWN,
                ))
            }
            Err(ArgError::UnscopedParameter { .. }) => {
                return Ok(deny(
                    checks,
                    0,
                    DenyReason::UnscopedParameter,
                    proposer_label.authority,
                    BTreeSet::new(),
                    RiskClass::UNKNOWN,
                ))
            }
        };
        checks.push(CheckRecord {
            enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
            step: 0,
            outcome: "pass",
            detail: "well_formed".to_string(),
        });

        // Step 1 — authority of proposal: eff = min(authority(proposer),
        // context_label.authority); taint = taint(ctx) ∪ taint(args).
        let ea = hh_provenance::effective_authority(
            proposer_label,
            &p.context_label,
            &args_prov.label(),
        );
        let eff = ea.authority;
        let taint = ea.taint;
        checks.push(CheckRecord {
            enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
            step: 1,
            outcome: "pass",
            detail: format!("eff={}", eff.as_str()),
        });

        // Step 2 — grant coverage and handle validity (check 2): live covering
        // handles by domain + scope over canonical params, attributes
        // narrowing, count/time constraints.
        let covering = self.resolve_handles(&p.proposer, p.effect.domain, &canonical);
        if covering.is_empty() {
            // A revoked covering handle reports `HandleRevoked`, not absence.
            let revoked_covers = self.table.handles.iter().any(|(_, h)| {
                h.holder.semantic_id == p.proposer
                    && h.validity.revoked_by.is_some()
                    && h.grants.iter().any(|g| {
                        g.effect.domain == p.effect.domain
                            && args::grant_scope_covers(&g.scope, &canonical)
                    })
            });
            return Ok(deny(
                checks,
                2,
                if revoked_covers {
                    DenyReason::HandleRevoked
                } else {
                    DenyReason::NoCoveringGrant
                },
                eff,
                taint,
                RiskClass::UNKNOWN,
            ));
        }
        // Attribute narrowing + constraints: the covering grant must cover the
        // declared class and have count/time headroom.
        let mut covered = Vec::new();
        let mut constraint_exhausted = false;
        for (id, g) in &covering {
            if !g.effect.covers(&p.effect) {
                continue; // attributes narrowing — not a cover for this class
            }
            let uses = self.table.uses.get(id).map(|u| u.decisions).unwrap_or(0);
            if let Some(max) = g.constraints.count {
                if uses >= max {
                    constraint_exhausted = true;
                    continue;
                }
            }
            if let Some(until) = g.constraints.time {
                if p.at >= until {
                    constraint_exhausted = true;
                    continue;
                }
            }
            covered.push(id.clone());
        }
        if covered.is_empty() {
            return Ok(deny(
                checks,
                2,
                if constraint_exhausted {
                    DenyReason::GrantConstraintExhausted
                } else {
                    DenyReason::NoCoveringGrant
                },
                eff,
                taint,
                RiskClass::UNKNOWN,
            ));
        }
        checks.push(CheckRecord {
            enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
            step: 2,
            outcome: "pass",
            detail: format!("covers={}", covered.len()),
        });

        // Step 3 — effect risk class: max_by_danger(kernel_assessed,
        // projected(declared), self_report) — the projection is inside
        // `kernel_assessed`; self-report is raise-only.
        let declared_attrs = declared_attributes(&cap.record, &p.effect);
        let kernel = assess::kernel_assessed(
            declared_attrs,
            p.effect.domain == EffectDomain::FsWrite,
            &p.inputs,
        );
        let risk = assess::apply_self_report(kernel, p.self_report);
        checks.push(CheckRecord {
            enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
            step: 3,
            outcome: "pass",
            detail: risk.to_string(),
        });

        // ── The C2 flow stage (§5g.2; R-2.8.2) ────────────────────────────
        // Runs between the risk assessment and Π's default row — before any
        // `ask` (D-ROBUST) and before Π (the rule tiers + check 3). When the
        // capability declares a `flow_contract`: `L⁺(p)` is computed and
        // joined into the decision's authority/taint (the contribution's
        // declared taint can only raise the check surface — join is monotone
        // restrictive); D-ROBUST refuses a tainted/`≤ external`
        // endorsement-surface input (`deny{RobustnessViolated}` +
        // `substitute` remedies); `check_flow` runs the contract's rules in
        // tier order (deny → allow/declassify/sanitize); check 3 enforces
        // `recipients(p) ⊆ readers(x)` on the egress domains (I-F4), a
        // coverage failure asking `{approval, sanitize}` (attended) or
        // denying `ReaderCoverage`. A `Fallthrough` hands Π's default row
        // the (possibly raised) `eff`/`taint` — the spec's tier order.
        let mut eff = eff;
        let mut taint = taint;
        let mut flow_verdict: Option<PiVerdict> = None;
        let mut flow_remedies: Vec<flow::Remedy> = Vec::new();
        if let Some(fc_json) = &cap.record.flow_contract {
            let contract = match FlowContract::from_json(fc_json) {
                Ok(c) => c,
                Err(e) => {
                    checks.push(CheckRecord {
                        step: 4,
                        outcome: "fail",
                        detail: format!("flow_contract_malformed:{}", e.detail),
                        enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                    });
                    return Ok(deny_rem(
                        checks,
                        4,
                        DenyReason::EvaluationError,
                        Vec::new(),
                        eff,
                        taint,
                        risk,
                    ));
                }
            };
            let fclass = contract.enforcement;
            let l_plus = flow::prospective_label(
                &p.context_label,
                p.flow.param_labels.values().cloned(),
                &contract.contribution,
                &p.capability_ref.semantic_id,
            );
            // Join L⁺ into the decision surface — never lowers (join is
            // monotone toward restrictive).
            eff = eff.min(l_plus.authority);
            taint = taint.union(&l_plus.taint).cloned().collect();
            checks.push(CheckRecord {
                step: 4,
                outcome: "pass",
                detail: format!("l_plus:{}", l_plus.authority.as_str()),
                enforcement: fclass,
            });
            // D-ROBUST (I-F2) — before any `ask`: the endorsement-surface
            // params (`recipient_params` ∪ sanitize-decision params) must be
            // `> external` and untainted or shape-endorsed.
            let mut robust_params = contract.recipient_params.clone();
            for r in &contract.rules {
                if let flow::FlowDecision::Sanitize { param, .. } = &r.decision {
                    if !robust_params.contains(param) {
                        robust_params.push(param.clone());
                    }
                }
            }
            let mut rinputs: Vec<flow::RobustnessInput> = Vec::new();
            let mut robust_fail: Vec<String> = Vec::new();
            for pn in &robust_params {
                match p.flow.param_labels.get(pn) {
                    Some(l) => rinputs.push(flow::RobustnessInput {
                        param: pn.clone(),
                        label: l,
                        shape_endorsed: p.flow.shape_endorsed.contains(pn),
                    }),
                    // An unrecorded label is unaccounted provenance — the
                    // input cannot be certified robust (fail-closed).
                    None => robust_fail.push(pn.clone()),
                }
            }
            if let Err(bad) = flow::d_robust(&rinputs) {
                robust_fail.extend(bad);
            }
            robust_fail.sort();
            robust_fail.dedup();
            if !robust_fail.is_empty() {
                checks.push(CheckRecord {
                    step: 4,
                    outcome: "fail",
                    detail: format!("d_robust:{}", robust_fail.join(",")),
                    enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                });
                let remedies = flow::enumerate_remedies(
                    &p.effect_id,
                    &[],
                    &robust_fail,
                    &contract,
                    self.policy.mode == Mode::Unattended,
                );
                return Ok(deny_rem(
                    checks,
                    4,
                    DenyReason::RobustnessViolated,
                    remedies,
                    eff,
                    taint,
                    risk,
                ));
            }
            // `recipients(p)` — resolved through `resolve_recipients` over
            // the canonical params; unresolvable on a contract that declares
            // recipient params is an evaluation failure (fail-closed).
            let recipients = match flow::resolve_recipients(&contract, &canonical.params) {
                Some(r) => r,
                None => {
                    checks.push(CheckRecord {
                        step: 4,
                        outcome: "fail",
                        detail: "recipients_unresolvable".to_string(),
                        enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                    });
                    return Ok(deny_rem(
                        checks,
                        4,
                        DenyReason::EvaluationError,
                        Vec::new(),
                        eff,
                        taint,
                        risk,
                    ));
                }
            };
            let world_open = p
                .effect
                .attributes
                .iter()
                .chain(declared_attrs.iter().copied())
                .any(|a| a.world == hh_hir::kinds::World::Open);
            let memory_ge_project = matches!(
                p.inputs.memory_scope,
                Some(MemoryScope::Project) | Some(MemoryScope::User)
            );
            let finput = FlowInput {
                l_plus: &l_plus,
                param_labels: &p.flow.param_labels,
                args: &canonical.params,
                recipients: &recipients,
                domain: p.effect.domain.name(),
                world: if world_open { "open" } else { "closed" },
                committed: &p.flow.committed,
                detectors: &p.flow.detectors,
                capability: &p.capability_ref.semantic_id,
            };
            let mut declassified_readers: Option<hh_provenance::ReaderSet> = None;
            match flow::check_flow(&contract.rules, &finput) {
                Err(e) => {
                    checks.push(CheckRecord {
                        step: 4,
                        outcome: "fail",
                        detail: format!("eval:{}", e.detail),
                        enforcement: fclass,
                    });
                    return Ok(deny_rem(
                        checks,
                        4,
                        DenyReason::EvaluationError,
                        Vec::new(),
                        eff,
                        taint,
                        risk,
                    ));
                }
                Ok(FlowVerdict::Deny {
                    detail,
                    reason,
                    remedies,
                }) => {
                    checks.push(CheckRecord {
                        step: 4,
                        outcome: "fail",
                        detail,
                        enforcement: fclass,
                    });
                    let reason = deny_reason_spelling(&reason);
                    return Ok(deny_rem(checks, 4, reason, remedies, eff, taint, risk));
                }
                Ok(FlowVerdict::Allow { rule_id }) => {
                    checks.push(CheckRecord {
                        step: 4,
                        outcome: "pass",
                        detail: format!("flow_allow:{rule_id}"),
                        enforcement: fclass,
                    });
                    flow_verdict = Some(PiVerdict::Allow);
                }
                Ok(FlowVerdict::Declassified {
                    rule_id,
                    readers_to,
                }) => {
                    checks.push(CheckRecord {
                        step: 4,
                        outcome: "pass",
                        detail: format!("flow_declassified:{rule_id}"),
                        enforcement: fclass,
                    });
                    declassified_readers = Some(readers_to);
                }
                Ok(FlowVerdict::Ask { detail, remedies }) => {
                    checks.push(CheckRecord {
                        step: 4,
                        outcome: "n/a",
                        detail,
                        enforcement: fclass,
                    });
                    flow_remedies = remedies;
                    flow_verdict = Some(PiVerdict::Ask);
                }
                Ok(FlowVerdict::Fallthrough) => {}
            }
            // Check 3 (I-F4) — `recipients(p) ⊆ readers(x)` for every
            // `content_param`; `Public` always passes. Runs on the egress
            // domain set unless a rule already decided the flow (an `ask`
            // stands; a `deny` returned above).
            if flow::check3_relevant(p.effect.domain.name(), world_open, memory_ge_project)
                && !matches!(flow_verdict, Some(PiVerdict::Ask))
            {
                let widened = declassified_readers.clone();
                let failures = flow::check_reader_coverage(&contract, &recipients, |x| {
                    if let Some(rs) = &widened {
                        return Some(rs);
                    }
                    p.flow.param_labels.get(x).map(|l| &l.readers)
                });
                if failures.is_empty() {
                    checks.push(CheckRecord {
                        step: 4,
                        outcome: "pass",
                        detail: "reader_coverage".to_string(),
                        enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                    });
                } else {
                    checks.push(CheckRecord {
                        step: 4,
                        outcome: "fail",
                        detail: format!("reader_coverage:{}", failures.join(",")),
                        enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                    });
                    let remedies = flow::enumerate_remedies(
                        &p.effect_id,
                        &failures,
                        &[],
                        &contract,
                        self.policy.mode == Mode::Unattended,
                    );
                    if remedies.is_empty() {
                        return Ok(deny_rem(
                            checks,
                            4,
                            DenyReason::ReaderCoverage,
                            Vec::new(),
                            eff,
                            taint,
                            risk,
                        ));
                    }
                    flow_remedies = remedies;
                    flow_verdict = Some(PiVerdict::Ask);
                }
            }
        }

        // Step 4 (authority-form half) — the Π gate: consulted when `eff ≤
        // external ∨ taint ≠ ∅` and the class is not `read_only ∧ closed`;
        // otherwise the ADR-0031 floor decides. Π-12's unattended transform
        // applies to both. A `flow_verdict` from the C2 stage skips the gate
        // (the contract's rule decided — Π's default row is the last tier).
        let read_only_closed = risk.reversibility == RiskReversibility::ReadOnly
            && risk.scope == RiskScope::WorkspaceLocal;
        let gate = eff <= AuthorityClass::External || !taint.is_empty();
        let (verdict, rows) = if let Some(fv) = flow_verdict {
            (fv, vec!["flow".to_string()])
        } else if gate && !read_only_closed {
            let ctx = PiContext {
                domain: p.effect.domain,
                risk,
                eff,
                tainted: !taint.is_empty(),
                inputs: p.inputs.clone(),
            };
            self.policy.evaluate(&ctx)
        } else {
            (
                floor_verdict(p.effect.domain, risk),
                vec!["floor".to_string()],
            )
        };
        // Π-12's unattended transform applies to the floor's `ask` too (the
        // Π path applies it internally — mirror it here so both agree).
        let verdict = if self.policy.mode == Mode::Unattended
            && verdict == PiVerdict::Ask
            && !p.inputs.pre_authorized.is_yes()
        {
            PiVerdict::Deny
        } else {
            verdict
        };
        checks.push(CheckRecord {
            enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
            step: 4,
            outcome: match verdict {
                PiVerdict::Allow => "pass",
                PiVerdict::Ask => "n/a",
                PiVerdict::Deny => "fail",
            },
            detail: if rows.is_empty() {
                "default".to_string()
            } else {
                rows.join(",")
            },
        });
        let handle_ids: Vec<String> = covered.iter().map(|h| h.as_str().to_string()).collect();
        match verdict {
            PiVerdict::Deny | PiVerdict::Ask | PiVerdict::Allow => {}
        }
        if let PiVerdict::Deny = verdict {
            // `UnattendedAsk` is the reason exactly when Π-12's transform
            // produced the deny — the Π path marks it by appending `pi_12`
            // to the matched-row list; the floor path's `ask → deny`
            // transform above is the same rule applied to the floor.
            let unattended_fired = rows.iter().any(|r| r == "pi_12")
                || (self.policy.mode == Mode::Unattended
                    && !p.inputs.pre_authorized.is_yes()
                    && rows.iter().any(|r| r == "floor"));
            let reason = if unattended_fired {
                DenyReason::UnattendedAsk
            } else {
                DenyReason::PolicyDenied
            };
            let mut c = checks;
            c.push(CheckRecord {
                enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                step: 4,
                outcome: "fail",
                detail: reason.as_str().to_string(),
            });
            return Ok(KernelDecision {
                effect_id: p.effect_id.clone(),
                decision: Decision::Deny {
                    reason,
                    remedies: Vec::new(),
                },
                effective_authority: eff,
                taint,
                effective_risk_class: risk,
                handle_ids,
                policy_ref: self.policy.version_id.clone(),
                checks: c,
                decider: Decider::Policy,
                decision_scope: DecisionScope::Once,
                cache_key: None,
                origin_permission_id: None,
                remedy_taken: None,
                assessment_inputs_ref: Some(assessment_inputs_ref(&p.inputs)),
            });
        }

        // Step 5 — the persistence ceiling (check 6): a `memory_write`/
        // `fs_write` with a scope above its store's write ceiling denies
        // `ScopeCeilingExceeded` — for `allow` and `ask` verdicts alike (a Π
        // row never overrides the ceiling).
        if let Some(detail) = self.persistence_ceiling_violation(p, &canonical, eff) {
            let mut c = checks;
            c.push(CheckRecord {
                enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                step: 5,
                outcome: "fail",
                detail: format!("scope_ceiling:{detail}"),
            });
            return Ok(KernelDecision {
                effect_id: p.effect_id.clone(),
                decision: Decision::Deny {
                    reason: DenyReason::ScopeCeilingExceeded,
                    remedies: Vec::new(),
                },
                effective_authority: eff,
                taint,
                effective_risk_class: risk,
                handle_ids,
                policy_ref: self.policy.version_id.clone(),
                checks: c,
                decider: Decider::Policy,
                decision_scope: DecisionScope::Once,
                cache_key: None,
                origin_permission_id: None,
                remedy_taken: None,
                assessment_inputs_ref: Some(assessment_inputs_ref(&p.inputs)),
            });
        }
        checks.push(CheckRecord {
            enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
            step: 5,
            outcome: "pass",
            detail: "persistence_ceiling".to_string(),
        });

        // Step 6 — delegation attenuation for `spawn_process`/`Delegate`: the
        // requested child grants must be ⊆ the covering handle's grants
        // (check 7's Stage-1 half — the minting side is [`crate::delegate`]).
        if p.effect.domain == EffectDomain::SpawnProcess {
            let parent_handle = &covered[0];
            let parent = self
                .table
                .get(parent_handle)
                .expect("covering handle is a row");
            for g in &p.requested_grants {
                if !parent
                    .grants
                    .iter()
                    .any(|pg| crate::delegate::grant_covers(pg, g))
                {
                    let mut c = checks;
                    c.push(CheckRecord {
                        enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                        step: 6,
                        outcome: "fail",
                        detail: DenyReason::AuthorityWidening.as_str().to_string(),
                    });
                    return Ok(KernelDecision {
                        effect_id: p.effect_id.clone(),
                        decision: Decision::Deny {
                            reason: DenyReason::AuthorityWidening,
                            remedies: Vec::new(),
                        },
                        effective_authority: eff,
                        taint,
                        effective_risk_class: risk,
                        handle_ids,
                        policy_ref: self.policy.version_id.clone(),
                        checks: c,
                        decider: Decider::Policy,
                        decision_scope: DecisionScope::Once,
                        cache_key: None,
                        origin_permission_id: None,
                        remedy_taken: None,
                        assessment_inputs_ref: Some(assessment_inputs_ref(&p.inputs)),
                    });
                }
            }
            checks.push(CheckRecord {
                enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                step: 6,
                outcome: "pass",
                detail: "attenuated".to_string(),
            });
        }

        // Steps 7–8 — approval feasibility then the lease cache (§2.2). The
        // exhaustion conversion applies to a *human-targeted* ask: a serving
        // lease resolves first (§5g.7's exhaustion row — "leases still
        // satisfy requests"), so the cache is evaluated before the budget
        // converts; the trail records them in spec order.
        if verdict == PiVerdict::Ask {
            let input = self.escalation_input(p, &canonical, cap, eff, risk);
            // A recorded decision covering this effect resolves the
            // re-dispatch — the suspended run resumed on `permission_decided`
            // (§5a.3 defer slice). The decided row is the truth: `allow`
            // serves with `decider = human` and the `origin_permission_id`
            // back-reference; `deny` reproduces the recorded verdict. The
            // recorded decision precedes the lease stage — a `decided` row
            // for this effect+attempt already exists, and re-minting one
            // would trip the exactly-one-decided gate (DuplicateDecision).
            if let Some((pid, rec)) = self.approvals.decision_for_effect(&p.effect_id) {
                match &rec.decision {
                    Decision::Allow => {
                        checks.push(CheckRecord {
                            enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                            step: 7,
                            outcome: "pass",
                            detail: format!("decided:{pid}"),
                        });
                        checks.push(CheckRecord {
                            enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                            step: 8,
                            outcome: "pass",
                            detail: format!("recorded:{pid}"),
                        });
                        return Ok(KernelDecision {
                            effect_id: p.effect_id.clone(),
                            decision: Decision::Allow,
                            effective_authority: eff,
                            taint,
                            effective_risk_class: risk,
                            handle_ids,
                            policy_ref: self.policy.version_id.clone(),
                            checks,
                            decider: Decider::Human,
                            decision_scope: DecisionScope::Once,
                            cache_key: None,
                            origin_permission_id: Some(pid.clone()),
                            remedy_taken: None,
                            assessment_inputs_ref: Some(assessment_inputs_ref(&p.inputs)),
                        });
                    }
                    Decision::Deny { reason, .. } => {
                        let mut c = checks;
                        c.push(CheckRecord {
                            enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                            step: 7,
                            outcome: "fail",
                            detail: format!("recorded_deny:{pid}"),
                        });
                        return Ok(KernelDecision {
                            effect_id: p.effect_id.clone(),
                            decision: Decision::Deny {
                                reason: *reason,
                                remedies: Vec::new(),
                            },
                            effective_authority: eff,
                            taint,
                            effective_risk_class: risk,
                            handle_ids,
                            policy_ref: self.policy.version_id.clone(),
                            checks: c,
                            decider: Decider::Human,
                            decision_scope: DecisionScope::Once,
                            cache_key: None,
                            origin_permission_id: Some(pid.clone()),
                            remedy_taken: None,
                            assessment_inputs_ref: Some(assessment_inputs_ref(&p.inputs)),
                        });
                    }
                    Decision::Ask { .. } => {}
                }
            }
            // The repeated-denial fallback (§5g.7 §5) — the sealed
            // `DenialPolicy` ceiling crossed on the `(capability_ref,
            // args_canonical_hash)` key converts the fresh ask to `deny`
            // (record-derived; never a widening, never an unbounded retry
            // loop). `stop_run`/`refuse_class` deny here; `escalate` leaves
            // the ask (the human stage re-renders once — the dispatcher's
            // escalation rows carry the hop).
            let denial_key = format!(
                "{}{}",
                input.capability_ref.version_id, input.args_canonical_hash
            );
            let denials = self
                .approvals
                .denial_counts
                .get(&denial_key)
                .copied()
                .unwrap_or(0);
            if let Some(pol) = &self.denial_policy {
                if denials > pol.max_denials
                    && !matches!(pol.fallback, crate::approval::DenialFallback::Escalate)
                {
                    let mut c = checks;
                    c.push(CheckRecord {
                        enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                        step: 7,
                        outcome: "fail",
                        detail: format!("denial_fallback:{}", pol.fallback.as_str()),
                    });
                    return Ok(KernelDecision {
                        effect_id: p.effect_id.clone(),
                        decision: Decision::Deny {
                            reason: DenyReason::PolicyDenied,
                            remedies: Vec::new(),
                        },
                        effective_authority: eff,
                        taint,
                        effective_risk_class: risk,
                        handle_ids,
                        policy_ref: self.policy.version_id.clone(),
                        checks: c,
                        decider: Decider::Policy,
                        decision_scope: DecisionScope::Once,
                        cache_key: None,
                        origin_permission_id: None,
                        remedy_taken: None,
                        assessment_inputs_ref: Some(assessment_inputs_ref(&p.inputs)),
                    });
                }
            }
            let hit = self.lease_hit(&input);
            let exhausted = self
                .approvals_max
                .map(|m| self.approvals.stats.requested >= m)
                .unwrap_or(false);
            if exhausted && hit.is_none() {
                let mut c = checks;
                c.push(CheckRecord {
                    enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                    step: 7,
                    outcome: "fail",
                    detail: DenyReason::ApprovalsExhausted.as_str().to_string(),
                });
                return Ok(KernelDecision {
                    effect_id: p.effect_id.clone(),
                    decision: Decision::Deny {
                        reason: DenyReason::ApprovalsExhausted,
                        remedies: Vec::new(),
                    },
                    effective_authority: eff,
                    taint,
                    effective_risk_class: risk,
                    handle_ids,
                    policy_ref: self.policy.version_id.clone(),
                    checks: c,
                    decider: Decider::Policy,
                    decision_scope: DecisionScope::Once,
                    cache_key: None,
                    origin_permission_id: None,
                    remedy_taken: None,
                    assessment_inputs_ref: Some(assessment_inputs_ref(&p.inputs)),
                });
            }
            checks.push(CheckRecord {
                enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                step: 7,
                outcome: "pass",
                detail: if hit.is_some() {
                    "reserved_or_leased".to_string()
                } else {
                    "reserved".to_string()
                },
            });
            if let Some((key, lease)) = hit {
                // Step 8 — the cache resolves `ask → allow`: `decider =
                // cache`, the lease key and `origin_permission_id` recorded,
                // the decision scope the lease's (I-P6/ADR-0071 D1).
                let mut c = checks;
                c.push(CheckRecord {
                    enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                    step: 8,
                    outcome: "pass",
                    detail: format!("lease:{key}"),
                });
                return Ok(KernelDecision {
                    effect_id: p.effect_id.clone(),
                    decision: Decision::Allow,
                    effective_authority: eff,
                    taint,
                    effective_risk_class: risk,
                    handle_ids,
                    policy_ref: self.policy.version_id.clone(),
                    checks: c,
                    decider: Decider::Cache,
                    decision_scope: match lease.scope {
                        approval::LeaseScope::Turn => DecisionScope::Once,
                        approval::LeaseScope::Run => DecisionScope::Session,
                        _ => DecisionScope::Persisted,
                    },
                    cache_key: Some(key),
                    origin_permission_id: Some(lease.origin_permission_id.clone()),
                    remedy_taken: None,
                    assessment_inputs_ref: Some(assessment_inputs_ref(&p.inputs)),
                });
            }
            checks.push(CheckRecord {
                enforcement: hh_provenance::flow::EnforcementClass::Deterministic,
                step: 8,
                outcome: "n/a",
                detail: "miss".to_string(),
            });
            return Ok(KernelDecision {
                effect_id: p.effect_id.clone(),
                decision: Decision::Ask {
                    options: stage1_ask_options(),
                    remedies: flow_remedies.clone(),
                },
                effective_authority: eff,
                taint,
                effective_risk_class: risk,
                handle_ids,
                policy_ref: self.policy.version_id.clone(),
                checks,
                decider: Decider::Policy,
                decision_scope: DecisionScope::Once,
                cache_key: None,
                origin_permission_id: None,
                remedy_taken: None,
                assessment_inputs_ref: Some(assessment_inputs_ref(&p.inputs)),
            });
        }

        Ok(KernelDecision {
            effect_id: p.effect_id.clone(),
            decision: Decision::Allow,
            effective_authority: eff,
            taint,
            effective_risk_class: risk,
            handle_ids,
            policy_ref: self.policy.version_id.clone(),
            checks,
            decider: Decider::Policy,
            decision_scope: DecisionScope::Once,
            cache_key: None,
            origin_permission_id: None,
            remedy_taken: None,
            assessment_inputs_ref: Some(assessment_inputs_ref(&p.inputs)),
        })
    }
}

/// The ADR-0031 floor (untainted `eff ≥ principal`): `allow` for every class
/// except `irreversible ∧ external`, `secret_access`, `spend`,
/// `permission_request` — `ask`.
pub fn floor_verdict(domain: EffectDomain, risk: RiskClass) -> PiVerdict {
    let floor_deny_ask =
        risk.reversibility == RiskReversibility::Irreversible && risk.scope == RiskScope::External;
    if floor_deny_ask
        || matches!(
            domain,
            EffectDomain::SecretAccess | EffectDomain::Spend | EffectDomain::PermissionRequest
        )
    {
        PiVerdict::Ask
    } else {
        PiVerdict::Allow
    }
}

/// The capability's declared `EffectAttributes` for the proposal's class —
/// `declared{set}` matches by domain; `pure` or an unmatched class is `None`
/// (→ `RiskClass::UNKNOWN` at the projection).
fn declared_attributes<'a>(
    t: &'a ToolCapabilityRecord,
    effect: &EffectClass,
) -> Option<&'a hh_hir::kinds::EffectAttributes> {
    match &t.effects {
        hh_hir::kinds::ToolEffects::Pure => None,
        hh_hir::kinds::ToolEffects::Declared(set) => set
            .iter()
            .find(|e| e.domain == effect.domain)
            .and_then(|e| e.attributes.as_ref()),
    }
}

/// `ReplayOutcome` — `replay`'s verdict: whether the re-derived decision
/// equals the recorded one, and the member names that differed (empty when
/// `matched`).
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayOutcome {
    /// Whether every compared member matched.
    pub matched: bool,
    /// The differing member names (the mismatch report — typed, not prose).
    pub mismatches: Vec<String>,
}

/// The `assessment_inputs_ref` coordinate — `H("assessment_inputs" ∥
/// canonical(inputs_json))`. The recorded inputs' identity; the reason is
/// reconstructible from the row it names (ADR-0066 D5).
pub fn assessment_inputs_ref(inputs: &AssessmentInputs) -> String {
    hh_identity::idp::idp_id(
        "assessment_inputs",
        assess::inputs_json(inputs).to_canonical_string().as_bytes(),
    )
}

/// Map a flow-rule `deny` decision's reason spelling onto the closed
/// `DenyReason` sum (§5g.2 §3 — the rule names a reason; an unregistered
/// spelling degrades to `PolicyDenied`, the Π-default refuse).
fn deny_reason_spelling(s: &str) -> DenyReason {
    match s {
        "MissingProvenance" => DenyReason::MissingProvenance,
        "UnmappedArgument" => DenyReason::UnmappedArgument,
        "UnscopedParameter" => DenyReason::UnscopedParameter,
        "UnknownProposer" => DenyReason::UnknownProposer,
        "NoCoveringGrant" => DenyReason::NoCoveringGrant,
        "GrantConstraintExhausted" => DenyReason::GrantConstraintExhausted,
        "HandleRevoked" => DenyReason::HandleRevoked,
        "PolicyDenied" => DenyReason::PolicyDenied,
        "ScopeCeilingExceeded" => DenyReason::ScopeCeilingExceeded,
        "AuthorityWidening" => DenyReason::AuthorityWidening,
        "NotDelegable" => DenyReason::NotDelegable,
        "BudgetExceedsParent" => DenyReason::BudgetExceedsParent,
        "ApprovalsExhausted" => DenyReason::ApprovalsExhausted,
        "UnattendedAsk" => DenyReason::UnattendedAsk,
        "ApprovalTimedOut" => DenyReason::ApprovalTimedOut,
        "ContainmentUnverified" => DenyReason::ContainmentUnverified,
        "containment" => DenyReason::Containment,
        "RobustnessViolated" => DenyReason::RobustnessViolated,
        "ReaderCoverage" => DenyReason::ReaderCoverage,
        "EvaluationError" => DenyReason::EvaluationError,
        _ => DenyReason::PolicyDenied,
    }
}

/// Parse a `PersistenceScope` spelling (`None` on any other value — never
/// coerced).
fn parse_persistence_scope(s: &str) -> Option<PersistenceScope> {
    match s {
        "definition" => Some(PersistenceScope::Definition),
        "user" => Some(PersistenceScope::User),
        "project" => Some(PersistenceScope::Project),
        "session" => Some(PersistenceScope::Session),
        "run" => Some(PersistenceScope::Run),
        "turn" => Some(PersistenceScope::Turn),
        _ => None,
    }
}

/// The Stage-1 `ask` options — ADR-0070 D1's `ApprovalOption.kind` set minus
/// `modify` (C2): `{allow_once, allow_lease, deny_once, deny_lease, abort_run,
/// escalate}`.
fn stage1_ask_options() -> Vec<String> {
    [
        "allow_once",
        "allow_lease",
        "deny_once",
        "deny_lease",
        "abort_run",
        "escalate",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}
