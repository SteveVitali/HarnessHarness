//! `authorize` — the monitor's decision function (§5g.1 §2.1/§2.2; ADR-0052
//! D1–D3). Steps **0–3 and 6** at Stage 1; step 4 lands as its authority-form
//! half (the Π gate + the ADR-0031 floor + Π-12's unattended transform, all
//! inside [`PolicyTable::evaluate`]/[`floor_verdict`]); steps 5, 7's lease/
//! approvals-budget halves and 8 are Stage-2 (the Stage-1 `ask` *is* step 7's
//! unconditional form — `ask` carries its options, the `ask → deny` unattended
//! transform is Π-12's).
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
use hh_provenance::{AuthorityClass, Label, ProvenanceRecord, TaintTag};
use hh_wire::json::Json;

use crate::args::{self, ArgError, CanonicalArgs};
use crate::assess::{self, AssessmentInputs};
use crate::decision::{CheckRecord, Decider, Decision, DenyReason, KernelDecision};
use crate::handle::HandleId;
use crate::policy::{Mode, PiContext, PiVerdict, PolicyTable};
use crate::table::HandleTable;

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
    /// The decision's logical time (`at` — the seq the `time` constraint's
    /// bound compares against).
    pub at: u64,
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
}

impl Monitor {
    /// A monitor over the given table and policy (empty registries — tests and
    /// the run-start path populate them).
    pub fn new(table: HandleTable, policy: PolicyTable) -> Monitor {
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
        }
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
                    step,
                    outcome: "fail",
                    detail: reason.as_str().to_string(),
                });
                c
            },
            decider: Decider::Policy,
        };
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
            step: 3,
            outcome: "pass",
            detail: risk.to_string(),
        });

        // Step 4 (authority-form half) — the Π gate: consulted when `eff ≤
        // external ∨ taint ≠ ∅` and the class is not `read_only ∧ closed`;
        // otherwise the ADR-0031 floor decides. Π-12's unattended transform
        // applies to both.
        let read_only_closed = risk.reversibility == RiskReversibility::ReadOnly
            && risk.scope == RiskScope::WorkspaceLocal;
        let gate = eff <= AuthorityClass::External || !taint.is_empty();
        let (verdict, rows) = if gate && !read_only_closed {
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
            PiVerdict::Deny => {
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
                });
            }
            PiVerdict::Ask => {
                return Ok(KernelDecision {
                    effect_id: p.effect_id.clone(),
                    decision: Decision::Ask {
                        options: stage1_ask_options(),
                        remedies: Vec::new(),
                    },
                    effective_authority: eff,
                    taint,
                    effective_risk_class: risk,
                    handle_ids,
                    policy_ref: self.policy.version_id.clone(),
                    checks,
                    decider: Decider::Policy,
                });
            }
            PiVerdict::Allow => {}
        }

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
                    });
                }
            }
            checks.push(CheckRecord {
                step: 6,
                outcome: "pass",
                detail: "attenuated".to_string(),
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
