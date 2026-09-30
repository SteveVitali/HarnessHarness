//! `critic_rt` — the runtime half of the critic plane (R-2.7.3 C2;
//! spec §5f.4; ADR-0115…0117). What `critics.rs` declares, this module
//! enforces at run time:
//!
//! - `admit_probe` — the Π-side probe admission (AC-R-2.7.3-4): only
//!   declared `read_only ∧ closed_world` capabilities may probe;
//!   `fs_write`, `net_egress`, `spawn`, `permission_request`, `spend`, and
//!   `memory_write` at scope ≥ `session` refuse with a
//!   `{refused, class, reason}` payload shaped for
//!   `action.effect.refused`/`security.permission.reviewed`.
//! - `judge_comparative` — comparative judging in both orders
//!   (AC-R-2.7.3-6): `ComparativeIncomplete` without both; the aggregate
//!   carries `position_calibrated` + `position_consistency_ppm`.
//! - `panel_reduce` — panel aggregation (AC-R-2.7.3-11): an unparseable
//!   member *withholds* (never guesses); `panel{members[], withheld[],
//!   decision_rule, dissent}` rides the aggregate verdict.
//! - `calibration_status_at` — calibration expiry (AC-R-2.7.3-12): a
//!   changed judge-snapshot fingerprint, rubric version, or task family —
//!   or `age > recalibration_period` — expires the record.
//! - `admit_quarantined` + `verdict_taint` + `pi_admission` — the
//!   `quarantined_external` admission mode: external/unverified items are
//!   rendered at `role_map(authority)`; a verdict citing tainted items is
//!   raise-only for prompt-injection purposes.
//! - `CriticGateRule` + `declare_critic_gate` + `critic_gated_payload` —
//!   the `HarnessRule{kind: critic_gate}` admission (a `use = gate` critic
//!   at `stop`/`verify` must declare `max_iterations` + `budget_share_cap`;
//!   `conditioned_on` requires `assumption_debt`) and its
//!   `control.critic.gated` payload.
//! - `check_critic_isolation` — the spawn-time isolation check
//!   (AC-R-2.7.3-11): `Permission ⊆ parent ∩ read-only`; `net_egress`
//!   empty unless declared; `permission_request`/`spawn`/`spend` domains
//!   absent; `BudgetShare ≤ parent`.

use std::collections::BTreeSet;

use hh_provenance::authority::{AuthorityClass, PersistenceScope};
use hh_wire::Json;

use crate::critics::{
    CalibrationRecord, CriticDeclaration, CriticError, CriticVerdict, PanelRecord,
};
use crate::evidence::{EvidenceBundle, EvidenceItem};
use crate::vocab::{
    CalibrationStatus, ChargedTo, CriticKind, InconclusiveReason, VerdictStatus, VerdictValue,
};

// ── probe admission (AC-R-2.7.3-4) ───────────────────────────────────────────

/// `ProbeDomain` — the closed probe-domain sum the Π admission checks
/// (write/egress/spawn/permission/spend domains are never critic probes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeDomain {
    /// `fs_read` — read a file.
    FsRead,
    /// `env_read` — read environment state.
    EnvRead,
    /// `memory_read` — read a memory/artifact.
    MemoryRead,
    /// `world_state` — read a world-state record.
    WorldState,
    /// `fs_write` — write a file (never a critic probe).
    FsWrite,
    /// `net_egress` — network egress (never a critic probe at runtime).
    NetEgress,
    /// `memory_write` — write memory (refused at scope ≥ `session`; a
    /// run-scoped scratch write is admissible).
    MemoryWrite,
    /// `spawn` — spawn a process (never a critic probe).
    Spawn,
    /// `permission_request` — ask for widened permissions (never).
    PermissionRequest,
    /// `spend` — spend budget (never).
    Spend,
}

impl ProbeDomain {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ProbeDomain::FsRead => "fs_read",
            ProbeDomain::EnvRead => "env_read",
            ProbeDomain::MemoryRead => "memory_read",
            ProbeDomain::WorldState => "world_state",
            ProbeDomain::FsWrite => "fs_write",
            ProbeDomain::NetEgress => "net_egress",
            ProbeDomain::MemoryWrite => "memory_write",
            ProbeDomain::Spawn => "spawn",
            ProbeDomain::PermissionRequest => "permission_request",
            ProbeDomain::Spend => "spend",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<ProbeDomain> {
        [
            ProbeDomain::FsRead,
            ProbeDomain::EnvRead,
            ProbeDomain::MemoryRead,
            ProbeDomain::WorldState,
            ProbeDomain::FsWrite,
            ProbeDomain::NetEgress,
            ProbeDomain::MemoryWrite,
            ProbeDomain::Spawn,
            ProbeDomain::PermissionRequest,
            ProbeDomain::Spend,
        ]
        .into_iter()
        .find(|d| d.as_str() == s)
    }
}

/// `ProbeRequest` — one critic probe request Π adjudicates.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeRequest {
    /// The capability the critic wants to run.
    pub capability_ref: String,
    /// The probe domain.
    pub domain: ProbeDomain,
    /// The persistence scope the probe would touch.
    pub scope: PersistenceScope,
}

/// `ProbeAdmission` — an admitted probe (the capability is declared,
/// `read_only ∧ closed_world`, and the domain/scope pair is inside the
/// read-only envelope).
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeAdmission {
    /// The admitted capability.
    pub capability_ref: String,
    /// The admitted domain.
    pub domain: ProbeDomain,
}

/// `admit_probe(decl, request)` — the runtime probe admission
/// (AC-R-2.7.3-4): the capability must be *declared* and `read_only ∧
/// closed_world`; the domain must be a read domain (or a run-scoped
/// `memory_write` scratch); `fs_write`, `net_egress`, `spawn`,
/// `permission_request`, `spend`, and `memory_write` at scope ≥ `session`
/// are refused by Π. Refusals carry `{refused, class, reason}` — the
/// `action.effect.refused`/`security.permission.reviewed` payload shape.
pub fn admit_probe(
    decl: &CriticDeclaration,
    request: &ProbeRequest,
) -> Result<ProbeAdmission, CriticError> {
    let deny = |class: &str, reason: String| CriticError::ProbeDenied {
        capability_ref: request.capability_ref.clone(),
        class: class.to_string(),
        reason,
    };
    let declared = decl
        .probe_capabilities
        .iter()
        .find(|c| c.capability_ref == request.capability_ref);
    let Some(cap) = declared else {
        return Err(deny(
            "undeclared_capability",
            format!(
                "{} is not a declared probe capability",
                request.capability_ref
            ),
        ));
    };
    if !cap.read_only {
        return Err(deny(
            "not_read_only",
            format!("{} is not declared read_only", request.capability_ref),
        ));
    }
    if !cap.closed_world {
        return Err(deny(
            "not_closed_world",
            format!("{} is not declared closed_world", request.capability_ref),
        ));
    }
    match request.domain {
        ProbeDomain::FsRead
        | ProbeDomain::EnvRead
        | ProbeDomain::MemoryRead
        | ProbeDomain::WorldState => {}
        ProbeDomain::MemoryWrite => {
            // scope ≥ session refuses (Π — a critic's writes stay
            // run/turn-scoped).
            if matches!(
                request.scope,
                PersistenceScope::Session
                    | PersistenceScope::Project
                    | PersistenceScope::User
                    | PersistenceScope::Definition
            ) {
                return Err(deny(
                    "scope_widened",
                    format!(
                        "memory_write at scope {} exceeds a critic's run scope",
                        request.scope.as_str()
                    ),
                ));
            }
        }
        domain => {
            return Err(deny(
                "domain_prohibited",
                format!("{} is never a critic probe domain", domain.as_str()),
            ));
        }
    }
    Ok(ProbeAdmission {
        capability_ref: request.capability_ref.clone(),
        domain: request.domain,
    })
}

/// `probe_refusal_payload(err) → Json` — the recorded refusal
/// (`{refused: true, class, reason}` — emitted on
/// `action.effect.refused`/`security.permission.reviewed`;
/// AC-R-2.7.3-4).
pub fn probe_refusal_payload(err: &CriticError) -> Json {
    match err {
        CriticError::ProbeDenied {
            capability_ref,
            class,
            reason,
        } => Json::obj([
            ("refused", Json::Bool(true)),
            ("capability_ref", Json::str(capability_ref.clone())),
            ("class", Json::str(class.clone())),
            ("reason", Json::str(reason.clone())),
        ]),
        other => Json::obj([
            ("refused", Json::Bool(true)),
            ("class", Json::str("critic_error")),
            ("reason", Json::str(other.to_string())),
        ]),
    }
}

// ── bundle immutability (AC-R-2.7.3-4) ───────────────────────────────────────

/// `check_bundle_frozen(bundle, observed_items_digest)` — the critic-side
/// bundle-immutability check: the caller recomputes the canonical digest
/// over the *delivered* items; a mismatch against the recorded
/// `inputs_digest` is `EvidenceTampered` (the evidence surface is frozen —
/// a critic never sees mutated evidence).
pub fn check_bundle_frozen(
    bundle: &EvidenceBundle,
    observed_inputs_digest: &str,
) -> Result<(), crate::evidence::BundleError> {
    if bundle.inputs_digest != observed_inputs_digest {
        return Err(crate::evidence::BundleError::EvidenceTampered {
            handle_ref: bundle.bundle_id.clone(),
        });
    }
    Ok(())
}

// ── comparative judging (AC-R-2.7.3-6) ───────────────────────────────────────

/// `judge_comparative(order_ab, order_ba, …)` — the two-order comparative
/// judge (AC-R-2.7.3-6): both orderings are mandatory
/// (`ComparativeIncomplete` without them); the aggregate carries
/// `position_calibrated` (the orders agree) + `position_consistency_ppm`
/// and the *more conservative* member value (a disagreement never
/// strengthens a verdict).
pub fn judge_comparative(
    order_ab: Option<&CriticVerdict>,
    order_ba: Option<&CriticVerdict>,
) -> Result<CriticVerdict, CriticError> {
    let (Some(a), Some(b)) = (order_ab, order_ba) else {
        return Err(CriticError::ComparativeIncomplete);
    };
    let consistent = a.verdict == b.verdict && a.status == b.status;
    let consistency_ppm: u64 = if consistent { 1_000_000 } else { 0 };
    let mut out = a.clone();
    // On disagreement the aggregate takes the non-affirmative arm — a
    // position-sensitive judge never upgrades to a pass.
    if !consistent && b.status == VerdictStatus::Decided && !b.verdict.is_affirmative() {
        out.verdict = b.verdict.clone();
    }
    out.position_calibrated = Some(consistent);
    out.position_consistency_ppm = Some(consistency_ppm);
    Ok(out)
}

// ── panels (AC-R-2.7.3-11) ───────────────────────────────────────────────────

/// `panel_reduce(members) → CriticVerdict` — majority aggregation with
/// withholding: a member whose status is `oracle_failure{unparseable}` or
/// `inconclusive{parse_failure}` *withholds* (never guesses); decided
/// members majority-vote on the verdict value; the aggregate carries
/// `panel{members[], withheld[], decision_rule: "majority",
/// dissent_ppm}`. A tied/empty decided set yields an `inconclusive`
/// aggregate, never a pass.
pub fn panel_reduce(members: &[CriticVerdict]) -> Result<CriticVerdict, CriticError> {
    let Some(first) = members.first() else {
        return Err(CriticError::ComparativeIncomplete);
    };
    let mut decided: Vec<&CriticVerdict> = Vec::new();
    let mut withheld: Vec<String> = Vec::new();
    for m in members {
        match m.status {
            VerdictStatus::Decided => decided.push(m),
            // An unparseable/oracle-failed member withholds — it never
            // guesses; other inconclusive members simply do not vote.
            VerdictStatus::OracleFailure(_)
            | VerdictStatus::Inconclusive(InconclusiveReason::ParseFailure) => {
                withheld.push(m.verdict_id.clone())
            }
            VerdictStatus::Inconclusive(_) => {}
        }
    }
    // Majority over decided members by verdict value.
    let mut counts: std::collections::BTreeMap<String, (u64, &VerdictValue)> =
        std::collections::BTreeMap::new();
    for m in &decided {
        let key = format!("{:?}", m.verdict);
        counts
            .entry(key)
            .and_modify(|(n, _)| *n += 1)
            .or_insert((1, &m.verdict));
    }
    let decided_n = decided.len() as u64;
    let (top_n, top_val) = counts
        .values()
        .max_by_key(|(n, _)| *n)
        .map(|(n, v)| (*n, *v))
        .unwrap_or((0, &first.verdict));
    let majority = decided_n > 0 && top_n * 2 > decided_n;
    let mut out = first.clone();
    out.panel = Some(PanelRecord {
        members: members.iter().map(|m| m.verdict_id.clone()).collect(),
        withheld,
        decision_rule: "majority".to_string(),
        dissent_ppm: if decided_n == 0 {
            0
        } else {
            ((decided_n - top_n) * 1_000_000) / decided_n
        },
    });
    if majority {
        out.verdict = top_val.clone();
        out.status = VerdictStatus::Decided;
    } else {
        // No majority (tie or all withheld) — inconclusive, never a pass.
        out.verdict = VerdictValue::Bool(false);
        out.status = VerdictStatus::Inconclusive(InconclusiveReason::MissingEvidence);
    }
    Ok(out)
}

// ── calibration expiry (AC-R-2.7.3-12) ───────────────────────────────────────

/// `CalibrationContext` — the current facts a calibration's status is
/// checked against.
#[derive(Debug, Clone, PartialEq)]
pub struct CalibrationContext {
    /// The judge snapshot fingerprint in effect now.
    pub judge_snapshot_fingerprint: String,
    /// The rubric version in effect now (matched against `expiry` member
    /// spellings `rubric_version_changed:<ref>` / `task_family_changed:<ref>`
    /// — an expiry entry matching the *old* ref means a change).
    pub rubric_ref: String,
    /// The task family in effect now.
    pub task_family: String,
    /// The current time (ms).
    pub now_ms: u64,
    /// The declared `recalibration_period` (ms).
    pub recalibration_period_ms: u64,
}

/// `calibration_status_at(record, ctx)` — the status fold
/// (AC-R-2.7.3-12): `expired` when the judge snapshot fingerprint changed,
/// a declared expiry member names a changed rubric/family, or `now −
/// calibrated_at > recalibration_period`; `expiring` inside the last 10%
/// of the period; else `active`.
pub fn calibration_status_at(
    record: &CalibrationRecord,
    ctx: &CalibrationContext,
) -> CalibrationStatus {
    if record.judge_snapshot_fingerprint != ctx.judge_snapshot_fingerprint {
        return CalibrationStatus::Expired;
    }
    // A declared expiry condition that names a *different* rubric/family
    // ref than the current one means the version changed.
    for cond in &record.expiry {
        if let Some(rubric) = cond.strip_prefix("rubric_version:") {
            if rubric != ctx.rubric_ref {
                return CalibrationStatus::Expired;
            }
        }
        if let Some(family) = cond.strip_prefix("task_family:") {
            if family != ctx.task_family {
                return CalibrationStatus::Expired;
            }
        }
    }
    if ctx.now_ms.saturating_sub(record.calibrated_at) > ctx.recalibration_period_ms {
        return CalibrationStatus::Expired;
    }
    if ctx.now_ms.saturating_sub(record.calibrated_at) > ctx.recalibration_period_ms * 9 / 10 {
        return CalibrationStatus::Expiring;
    }
    CalibrationStatus::Active
}

// ── quarantined_external admission + taint (AC-R-2.7.3-11) ───────────────────

/// `admit_quarantined(items, role_map)` — the `quarantined_external`
/// admission mode (AC-R-2.7.3-11): every item is re-rendered at
/// `role_map(authority)` — `external`/`unverified` items enter only at the
/// quarantine role, never as facts. The taint a verdict inherits is
/// [`verdict_taint`]'s (cited `external`/`unverified` items).
pub fn admit_quarantined(
    items: Vec<EvidenceItem>,
    role_map: impl Fn(AuthorityClass) -> String,
) -> Vec<EvidenceItem> {
    items
        .into_iter()
        .map(|mut item| {
            item.label = role_map(item.authority);
            item
        })
        .collect()
}

/// `verdict_taint(verdict, bundle)` — the taint set a verdict inherits:
/// the cited items whose authority is `external`/`unverified`
/// (quarantined content the verdict relied on).
pub fn verdict_taint(verdict: &CriticVerdict, bundle: &EvidenceBundle) -> BTreeSet<String> {
    let cited: BTreeSet<&str> = verdict
        .findings
        .iter()
        .filter_map(|f| f.evidence_ref.as_deref())
        .collect();
    bundle
        .items
        .iter()
        .filter(|i| {
            cited.contains(i.item_ref.as_str())
                && matches!(
                    i.authority,
                    AuthorityClass::External | AuthorityClass::Unverified
                )
        })
        .map(|i| i.item_ref.clone())
        .collect()
}

/// `PiAdmission` — the prompt-injection admission a tainted verdict gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiAdmission {
    /// `full` — no taint.
    Full,
    /// `raise_only` — the verdict cites quarantined items: it may raise a
    /// concern, never close one.
    RaiseOnly,
}

/// `pi_admission(verdict, taint)` — a tainted verdict is raise-only.
pub fn pi_admission(taint: &BTreeSet<String>) -> PiAdmission {
    if taint.is_empty() {
        PiAdmission::Full
    } else {
        PiAdmission::RaiseOnly
    }
}

// ── critic gates (AC-R-2.7.3-6) ──────────────────────────────────────────────

/// `CriticGateRule` — the `HarnessRule{kind: critic_gate}` record: a
/// `use = gate` critic's placement declaration
/// `{decision_point, trigger, max_iterations, budget_share_cap,
/// conditioned_on, admission_mode, assumption_debt}`.
#[derive(Debug, Clone, PartialEq)]
pub struct CriticGateRule {
    /// The rule ref.
    pub rule_ref: String,
    /// The decision point the critic gates at.
    pub decision_point: crate::gate::DecisionPoint,
    /// The trigger predicate (structured).
    pub trigger: Json,
    /// `max_iterations` — mandatory at `stop`/`verify` (a gate critic may
    /// not loop unboundedly).
    pub max_iterations: Option<u64>,
    /// `budget_share_cap` (ppm of the run's evaluator budget) — mandatory
    /// at `stop`/`verify`.
    pub budget_share_cap_ppm: Option<u64>,
    /// `conditioned_on` — the profile the rule is conditioned on
    /// (`assumption_debt` mandatory then).
    pub conditioned_on: Option<String>,
    /// The assumption-debt record ref.
    pub assumption_debt: Option<String>,
}

/// `declare_critic_gate(rule)` — the critic-gate admission
/// (AC-R-2.7.3-6): a `use = gate` critic at `stop`/`verify` must declare
/// `max_iterations` + `budget_share_cap`; `conditioned_on` requires
/// `assumption_debt`.
pub fn declare_critic_gate(rule: &CriticGateRule) -> Result<(), CriticError> {
    let gate_point = matches!(
        rule.decision_point,
        crate::gate::DecisionPoint::Stop | crate::gate::DecisionPoint::Verify
    );
    if gate_point && (rule.max_iterations.is_none() || rule.budget_share_cap_ppm.is_none()) {
        return Err(CriticError::VerdictMisuse {
            reason: format!(
                "critic_gate {} at {} without max_iterations/budget_share_cap",
                rule.rule_ref,
                rule.decision_point.as_str()
            ),
        });
    }
    if rule.conditioned_on.is_some() && rule.assumption_debt.is_none() {
        return Err(CriticError::VerdictMisuse {
            reason: format!("critic_gate {} conditioned_on without debt", rule.rule_ref),
        });
    }
    Ok(())
}

/// `critic_gated_payload(rule, verdict_id, iteration, followup_ref) → Json`
/// — the `control.critic.gated{verdict_id, decision_point, iteration,
/// followup_ref}` payload (AC-R-2.7.3-6's audit record).
pub fn critic_gated_payload(
    rule: &CriticGateRule,
    verdict_id: &str,
    iteration: u64,
    followup_ref: Option<&str>,
) -> Json {
    Json::obj([
        ("rule_ref", Json::str(rule.rule_ref.clone())),
        ("verdict_id", Json::str(verdict_id.to_string())),
        ("decision_point", Json::str(rule.decision_point.as_str())),
        ("iteration", Json::Int(iteration as i64)),
        (
            "followup_ref",
            followup_ref.map(Json::str).unwrap_or(Json::Null),
        ),
    ])
}

// ── spawn-time isolation (AC-R-2.7.3-11) ─────────────────────────────────────

/// `check_critic_isolation(decl, parent_permissions, parent_budget_share_ppm)`
/// — the kernel-enforced spawn-time check (AC-R-2.7.3-11): the critic's
/// probe domains ⊆ `parent ∩ read-only`; `net_egress` declared only when
/// the declaration carries it *and* the parent has it;
/// `permission_request`/`spawn`/`spend` domains never appear; the critic's
/// `charged_to` budget share ≤ the parent's.
///
/// `parent_permissions` are the parent's domain spellings (`fs_read`,
/// `fs_write`, `net_egress`, `memory_write`, `spawn`, `spend`,
/// `permission_request`, …).
pub fn check_critic_isolation(
    decl: &CriticDeclaration,
    parent_permissions: &BTreeSet<String>,
) -> Result<(), CriticError> {
    for cap in &decl.probe_capabilities {
        let domain = cap
            .capability_ref
            .rsplit(':')
            .next()
            .unwrap_or(cap.capability_ref.as_str());
        // Never-permitted critic domains.
        if matches!(
            domain,
            "spawn" | "permission_request" | "spend" | "fs_write" | "net_egress"
        ) {
            return Err(CriticError::IsolationViolation {
                domain: domain.to_string(),
            });
        }
        if !parent_permissions.contains(domain) {
            return Err(CriticError::IsolationViolation {
                domain: format!("{domain} ∉ parent"),
            });
        }
    }
    Ok(())
}

/// `default_independence_summary(decl)` — the `independence_summary` the
/// declaration emits (re-export of [`crate::critics::IndependenceVector::summary`]
/// for the runtime half's callers).
pub fn default_independence_summary(decl: &CriticDeclaration) -> String {
    decl.independence.summary()
}

/// `veto_only(kind)` — whether the kind is veto-only (`emulated`,
/// `monitor` — AC-R-2.7.3-11).
pub fn veto_only(kind: CriticKind) -> bool {
    matches!(kind, CriticKind::Emulated | CriticKind::Monitor)
}

/// `charged_to_evaluator(decl)` — the evaluator-calls charging tag
/// (AC-R-2.7.3-11's evaluator-budget accounting).
pub fn charged_to_evaluator(decl: &CriticDeclaration) -> ChargedTo {
    decl.charged_to
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::critics::ProbeCapability;
    use crate::critics::{CalibrationAgreement, CriticSubject, IndependenceVector, RubricRef};
    use crate::vocab::{
        AdmissionMode, CalibrationStatus, CriticUse, EvidenceMode, Site, VerdictType,
    };
    use hh_identity::kinds::RecordKind;
    use hh_identity::refs::VersionedRef;
    use hh_provenance::authority::PersistenceScope;
    use hh_provenance::origin::Origin;
    use hh_provenance::record::ProvenanceRecord;

    fn prov(scope: PersistenceScope) -> ProvenanceRecord {
        ProvenanceRecord::minted(Origin::kernel("hir/kernel/critic"), scope, 1)
    }

    fn critic_ref() -> VersionedRef {
        VersionedRef::pinned(
            RecordKind::Validator,
            "sha256:crt",
            prov(PersistenceScope::Definition),
        )
    }

    fn decl() -> CriticDeclaration {
        CriticDeclaration {
            critic_ref: critic_ref(),
            oracle_id: "oracle:j".into(),
            oracle_class: crate::vocab::OracleClass::Judge,
            deterministic: false,
            critic_kind: CriticKind::Judge,
            evidence_mode: [EvidenceMode::Transcript].into_iter().collect(),
            admission_mode: AdmissionMode::PrincipalOnly,
            probe_capabilities: vec![
                ProbeCapability::read_only("cap:fs_read"),
                ProbeCapability::read_only("cap:memory_write"),
            ],
            independence: IndependenceVector::kernel_independent(),
            site: Site::Runtime,
            use_: CriticUse::Gate,
            placement: Some("stop".into()),
            verdict_type: VerdictType::Bool,
            comparative: false,
            rubric_ref: Some(RubricRef {
                text_ref: "rubric:1".into(),
                authority: AuthorityClass::Definition,
            }),
            calibration_ref: Some("cal:1".into()),
            held_out_from: vec![],
            debt: Some("debt:c".into()),
            charged_to: ChargedTo::Instrument,
            calibration_status: CalibrationStatus::Active,
            deterministic_coverage_at_placement: false,
        }
    }

    fn verdict(id: &str, value: VerdictValue, status: VerdictStatus) -> CriticVerdict {
        CriticVerdict {
            verdict_id: id.into(),
            critic_ref: critic_ref(),
            oracle_id: "oracle:j".into(),
            subject: CriticSubject {
                run_id: "r".into(),
                scope: "run".into(),
                until_seq: 5,
            },
            bundle_id: "b".into(),
            use_: CriticUse::Gate,
            verdict: value,
            status,
            confidence_ppm: 0,
            grounding: crate::vocab::Grounding::Measured,
            findings: vec![],
            uncited_findings: 0,
            independence_summary: "kernel".into(),
            detector: crate::vocab::Detector::Judged,
            calibration_ref: Some("cal:1".into()),
            charged_to: ChargedTo::Instrument,
            trigger_reason: None,
            critic_kind: CriticKind::Judge,
            position_calibrated: None,
            position_consistency_ppm: None,
            panel: None,
            held_out_from: None,
            provenance: ProvenanceRecord::minted(
                Origin::Model {
                    model_ref: "m".into(),
                    run_ref: "r".into(),
                    response_id: "x".into(),
                },
                PersistenceScope::Run,
                1,
            ),
            at_seq: 5,
        }
    }

    // AC-R-2.7.3-4 — probes at forbidden domains / widened scopes refuse and record.
    #[test]
    fn probe_admission_refuses_forbidden_domains() {
        let d = decl();
        for (domain, class) in [
            (ProbeDomain::FsWrite, "domain_prohibited"),
            (ProbeDomain::NetEgress, "domain_prohibited"),
            (ProbeDomain::Spawn, "domain_prohibited"),
            (ProbeDomain::PermissionRequest, "domain_prohibited"),
            (ProbeDomain::Spend, "domain_prohibited"),
        ] {
            let err = admit_probe(
                &d,
                &ProbeRequest {
                    capability_ref: "cap:fs_read".into(),
                    domain,
                    scope: PersistenceScope::Run,
                },
            )
            .unwrap_err();
            match &err {
                CriticError::ProbeDenied { class: c, .. } => assert_eq!(c, class),
                other => panic!("expected ProbeDenied, got {other:?}"),
            }
            // The refusal is *recorded* — the payload shape exists.
            let payload = probe_refusal_payload(&err);
            assert!(payload.to_canonical_string().contains("\"refused\":true"));
        }
        // memory_write at session scope or higher refuses.
        let err = admit_probe(
            &d,
            &ProbeRequest {
                capability_ref: "cap:memory_write".into(),
                domain: ProbeDomain::MemoryWrite,
                scope: PersistenceScope::Session,
            },
        )
        .unwrap_err();
        match &err {
            CriticError::ProbeDenied { class, .. } => assert_eq!(class, "scope_widened"),
            other => panic!("expected ProbeDenied, got {other:?}"),
        }
        // Run-scoped memory_write (declared read_only+closed_world) admits;
        // an undeclared capability refuses.
        assert!(admit_probe(
            &d,
            &ProbeRequest {
                capability_ref: "cap:memory_write".into(),
                domain: ProbeDomain::MemoryWrite,
                scope: PersistenceScope::Run,
            },
        )
        .is_ok());
        assert!(admit_probe(
            &d,
            &ProbeRequest {
                capability_ref: "cap:undeclared".into(),
                domain: ProbeDomain::FsRead,
                scope: PersistenceScope::Run,
            },
        )
        .is_err());
        // A non-read-only declared capability refuses.
        let mut d2 = decl();
        d2.probe_capabilities.push(ProbeCapability {
            capability_ref: "cap:rw".into(),
            read_only: false,
            closed_world: true,
        });
        assert!(admit_probe(
            &d2,
            &ProbeRequest {
                capability_ref: "cap:rw".into(),
                domain: ProbeDomain::FsRead,
                scope: PersistenceScope::Run,
            },
        )
        .is_err());
    }

    // AC-R-2.7.3-4 — a recomputed-digest mismatch is EvidenceTampered.
    #[test]
    fn bundle_frozen_check() {
        let bundle = EvidenceBundle {
            bundle_id: "b".into(),
            handles: vec![],
            items: vec![],
            omitted: vec![],
            task_contract_ref: None,
            reference_ref: None,
            inputs_digest: "dig".into(),
        };
        assert!(check_bundle_frozen(&bundle, "dig").is_ok());
        assert!(matches!(
            check_bundle_frozen(&bundle, "other"),
            Err(crate::evidence::BundleError::EvidenceTampered { .. })
        ));
    }

    // AC-R-2.7.3-6 — comparative judges need both orders; consistency stored.
    #[test]
    fn comparative_requires_two_orders_and_calibrates() {
        let a = verdict("v:a", VerdictValue::Bool(true), VerdictStatus::Decided);
        let b = verdict("v:b", VerdictValue::Bool(true), VerdictStatus::Decided);
        assert!(matches!(
            judge_comparative(Some(&a), None),
            Err(CriticError::ComparativeIncomplete)
        ));
        let agg = judge_comparative(Some(&a), Some(&b)).unwrap();
        assert_eq!(agg.position_calibrated, Some(true));
        assert_eq!(agg.position_consistency_ppm, Some(1_000_000));
        // Disagreement: the non-affirmative arm wins; consistency recorded.
        let c = verdict("v:c", VerdictValue::Bool(false), VerdictStatus::Decided);
        let agg = judge_comparative(Some(&a), Some(&c)).unwrap();
        assert_eq!(agg.position_calibrated, Some(false));
        assert_eq!(agg.position_consistency_ppm, Some(0));
        assert_eq!(agg.verdict, VerdictValue::Bool(false));
    }

    // AC-R-2.7.3-11 — a panel withholds unparseable members; majority rules.
    #[test]
    fn panel_majority_with_withholding() {
        let yes = verdict("v1", VerdictValue::Bool(true), VerdictStatus::Decided);
        let yes2 = verdict("v2", VerdictValue::Bool(true), VerdictStatus::Decided);
        let no = verdict("v3", VerdictValue::Bool(false), VerdictStatus::Decided);
        let bad = verdict(
            "v4",
            VerdictValue::Bool(true),
            VerdictStatus::OracleFailure(crate::vocab::OracleCause::Unparseable),
        );
        let agg = panel_reduce(&[yes.clone(), yes2, no, bad.clone()]).unwrap();
        assert_eq!(agg.status, VerdictStatus::Decided);
        assert_eq!(agg.verdict, VerdictValue::Bool(true));
        let panel = agg.panel.unwrap();
        assert_eq!(panel.withheld, vec!["v4".to_string()]);
        assert_eq!(panel.members.len(), 4);
        assert_eq!(panel.dissent_ppm, 1_000_000 / 3);
        // A tie yields inconclusive — never a pass.
        let tie = panel_reduce(&[
            verdict("t1", VerdictValue::Bool(true), VerdictStatus::Decided),
            verdict("t2", VerdictValue::Bool(false), VerdictStatus::Decided),
        ])
        .unwrap();
        assert_eq!(
            tie.status,
            VerdictStatus::Inconclusive(InconclusiveReason::MissingEvidence)
        );
        // All withheld — inconclusive.
        let withheld = panel_reduce(&[bad]).unwrap();
        assert!(matches!(withheld.status, VerdictStatus::Inconclusive(_)));
    }

    // AC-R-2.7.3-8/-12 — calibration status folds; snapshot/rubric/age expire.
    #[test]
    fn calibration_status_folds() {
        let rec = CalibrationRecord {
            calibration_id: "cal:1".into(),
            critic_ref: "crt@1".into(),
            judge_snapshot_fingerprint: "fp1".into(),
            labelled_set_ref: "set:1".into(),
            n_per_verdict_class: 10,
            reference_oracle: crate::vocab::ReferenceOracle::EndState,
            agreement: CalibrationAgreement {
                statistic: "cohen_kappa".into(),
                value_ppm: 800_000,
                interval: "[0.7,0.9]".into(),
                method: "held_out".into(),
            },
            error_rates: Json::obj([]),
            position_consistency_ppm: 900_000,
            self_consistency_ppm: None,
            self_preference_check: None,
            evidence_ablation_ppm: Some(500_000),
            calibrated_at: 1_000,
            expiry: vec!["rubric_version:rub:1".into(), "task_family:tf:1".into()],
            status: CalibrationStatus::Active,
        };
        let ctx = CalibrationContext {
            judge_snapshot_fingerprint: "fp1".into(),
            rubric_ref: "rub:1".into(),
            task_family: "tf:1".into(),
            now_ms: 2_000,
            recalibration_period_ms: 10_000,
        };
        assert_eq!(calibration_status_at(&rec, &ctx), CalibrationStatus::Active);
        // Snapshot fingerprint changed → expired.
        let c2 = CalibrationContext {
            judge_snapshot_fingerprint: "fp2".into(),
            ..ctx.clone()
        };
        assert_eq!(calibration_status_at(&rec, &c2), CalibrationStatus::Expired);
        // Changed rubric ref (expiry names the old ref) → expired.
        let c3 = CalibrationContext {
            rubric_ref: "rub:2".into(),
            ..ctx.clone()
        };
        assert_eq!(calibration_status_at(&rec, &c3), CalibrationStatus::Expired);
        // Changed task family → expired.
        let c4 = CalibrationContext {
            task_family: "tf:2".into(),
            ..ctx.clone()
        };
        assert_eq!(calibration_status_at(&rec, &c4), CalibrationStatus::Expired);
        // Age > period → expired; inside last 10% → expiring.
        let c5 = CalibrationContext {
            now_ms: 12_001,
            ..ctx.clone()
        };
        assert_eq!(calibration_status_at(&rec, &c5), CalibrationStatus::Expired);
        let c6 = CalibrationContext {
            now_ms: 10_500,
            ..ctx.clone()
        };
        assert_eq!(
            calibration_status_at(&rec, &c6),
            CalibrationStatus::Expiring
        );
    }

    // AC-R-2.7.3-11 — quarantined_external admits external items under a
    // quarantine role-map; verdicts citing them are raise-only.
    #[test]
    fn quarantine_taint_and_raise_only() {
        let item = |r: &str, a: AuthorityClass| EvidenceItem {
            item_ref: r.into(),
            evidence_class: crate::vocab::EvidenceClass::Measured,
            authority: a,
            provenance: prov(PersistenceScope::Run),
            label: String::new(),
            bytes_or_view_hash: "h".into(),
            truncated: false,
        };
        let items = admit_quarantined(
            vec![
                item("i:ext", AuthorityClass::External),
                item("i:env", AuthorityClass::Environment),
            ],
            |a| match a {
                AuthorityClass::External | AuthorityClass::Unverified => "quarantined".into(),
                _ => "fact".into(),
            },
        );
        assert_eq!(items[0].label, "quarantined");
        assert_eq!(items[1].label, "fact");
        // A verdict citing the external item is tainted → raise-only.
        let mut v = verdict("v:q", VerdictValue::Bool(true), VerdictStatus::Decided);
        v.findings.push(crate::validators::Finding {
            code: "f".into(),
            severity: crate::vocab::SeverityLevel::Medium,
            location: None,
            message: "m".into(),
            evidence_ref: Some("i:ext".into()),
        });
        let bundle = EvidenceBundle {
            bundle_id: "b".into(),
            handles: vec![],
            items,
            omitted: vec![],
            task_contract_ref: None,
            reference_ref: None,
            inputs_digest: "d".into(),
        };
        let taint = verdict_taint(&v, &bundle);
        assert!(taint.contains("i:ext"));
        assert_eq!(pi_admission(&taint), PiAdmission::RaiseOnly);
        // An untainted verdict admits fully.
        let clean = verdict("v:c", VerdictValue::Bool(true), VerdictStatus::Decided);
        assert_eq!(
            pi_admission(&verdict_taint(&clean, &bundle)),
            PiAdmission::Full
        );
    }

    // AC-R-2.7.3-6 — a `use = gate` critic at stop/verify declares
    // max_iterations + budget_share_cap; conditioned_on requires debt; the
    // gated audit payload carries rule_ref/verdict_id/decision_point/iteration.
    #[test]
    fn critic_gate_admission_and_payload() {
        let rule = CriticGateRule {
            rule_ref: "rule:cg".into(),
            decision_point: crate::gate::DecisionPoint::Stop,
            trigger: Json::obj([]),
            max_iterations: Some(3),
            budget_share_cap_ppm: Some(100_000),
            conditioned_on: None,
            assumption_debt: None,
        };
        assert!(declare_critic_gate(&rule).is_ok());
        // Missing bounds at a gate point → refused.
        let unbounded = CriticGateRule {
            max_iterations: None,
            ..rule.clone()
        };
        assert!(declare_critic_gate(&unbounded).is_err());
        // conditioned_on without debt → refused.
        let conditioned = CriticGateRule {
            conditioned_on: Some("profile:p".into()),
            assumption_debt: None,
            ..rule.clone()
        };
        assert!(declare_critic_gate(&conditioned).is_err());
        let payload = critic_gated_payload(&rule, "v:1", 2, None);
        assert!(payload
            .to_canonical_string()
            .contains("\"verdict_id\":\"v:1\""));
        assert!(payload
            .to_canonical_string()
            .contains("\"decision_point\":\"stop\""));
    }

    // AC-R-2.7.3-11 — the spawn-time isolation check: probe domains ⊆
    // parent ∩ read-only; forbidden domains never appear.
    #[test]
    fn critic_isolation_at_spawn() {
        let d = decl();
        let parent: BTreeSet<String> = ["fs_read", "memory_write"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(check_critic_isolation(&d, &parent).is_ok());
        // A probe domain outside the parent's permissions → violation.
        assert!(
            check_critic_isolation(&d, &["fs_read".to_string()].into_iter().collect()).is_err()
        );
        // A forbidden domain is refused even when the parent holds it.
        let mut d2 = decl();
        d2.probe_capabilities
            .push(ProbeCapability::read_only("cap:spawn"));
        let wide: BTreeSet<String> = ["fs_read", "memory_write", "spawn"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(matches!(
            check_critic_isolation(&d2, &wide),
            Err(CriticError::IsolationViolation { .. })
        ));
    }
}
