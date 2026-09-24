//! `gate` — the completion-gate substrate (spec §5f.1 §3, §5f.2 §3; ADR-0109,
//! ADR-0113): `ValidatesRecord`, `AcceptanceCriterion`, `TaskContract`,
//! `VerificationSummary`, `GateResult`, the Γ intervention table with the
//! kernel floors F2–F7 as constants, `validate_gamma` (tighten-only), and the
//! pure `evaluate_gate` fold. Every evaluation appends
//! `verification.gate.evaluated`; `hold_count` is a ledger fact so re-entry
//! loops are impossible by construction (F4).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_identity::idp::idp_id;
use hh_wire::Json;

use crate::claims::CriterionState;
use crate::validators::EvidenceRequirement;
use crate::vocab::{
    CompletionPolicy, CriterionRole, Detector, DivergenceClass, EvidenceKind, GateVerdict,
    Intervention, OracleCause, SeverityLevel, VerdictPhase, Visibility, STRATUM_FAILED_HONESTLY,
    STRATUM_UNRECONCILED_CLAIMS, VETO_FALSE_COMPLETION,
};

// ── ValidatesRecord / AcceptanceCriterion / TaskContract (ADR-0109 D1/D2) ───

/// `ValidatesRecord` — the typed fields on a `validates` edge (a
/// verification-plane record of this data model per the OQ-469 ratified
/// default — *not* an HIR/1 edge attribute).
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatesRecord {
    /// The criterion role.
    pub role: CriterionRole,
    /// The placement phase.
    pub phase: VerdictPhase,
    /// Whether the criterion is `required` for completion.
    pub required: bool,
    /// The visibility (`held_out` is never delivered — `HeldOutLeak`).
    pub visibility: Visibility,
    /// Whether a fail trips a veto invariant.
    pub veto: bool,
    /// The evidence requirements.
    pub evidence_requirements: Vec<EvidenceRequirement>,
    /// An optional window restriction.
    pub window: Option<String>,
    /// An optional weight (`weighted_threshold` policies).
    pub weight_ppm: Option<u64>,
}

/// `AcceptanceCriterion` — one `TaskContract.criteria[]` member
/// (`{criterion_id, validator_ref: VersionedRef, record: ValidatesRecord}` —
/// ADR-0109 D1).
#[derive(Debug, Clone, PartialEq)]
pub struct AcceptanceCriterion {
    /// The criterion id (aligned to `Goal.success_criteria` — a ref, never an
    /// ordinal alone).
    pub criterion_id: String,
    /// The pinned validator ref (`version_id` — an unpinned criterion is
    /// `UnboundFixture` upstream).
    pub validator_ref: String,
    /// The `validates` record.
    pub record: ValidatesRecord,
}

/// `TaskContract` — the typed projection over `Goal`, its `success_criteria`
/// and `validates` records (ADR-0109 D1 — *not* an HIR entity; computed at
/// `seal`, never edited during a run).
#[derive(Debug, Clone, PartialEq)]
pub struct TaskContract {
    /// `contract_id = H(canonical(goal.semantic_id ∥ sorted criterion refs ∥
    /// records))` — stamped in the run manifest as `task_contract_id`.
    pub contract_id: String,
    /// The goal ref the contract projects.
    pub goal_ref: String,
    /// The acceptance criteria.
    pub criteria: Vec<AcceptanceCriterion>,
    /// The run invariants.
    pub invariants: Vec<AcceptanceCriterion>,
    /// The completion policy.
    pub completion_policy: CompletionPolicy,
    /// The evidence kinds the contract requires.
    pub evidence_kinds_required: Vec<EvidenceKind>,
    /// The budget ref.
    pub budget_ref: String,
    /// Whether the contract is sealed (an unsealed contract is a draft).
    pub sealed: bool,
}

impl TaskContract {
    /// The `contract_id` construction — `H(canonical(goal.semantic_id ∥ sorted
    /// criterion refs ∥ records))` (ADR-0109 D1).
    pub fn compute_contract_id(goal_semantic_id: &str, criteria: &[AcceptanceCriterion]) -> String {
        let mut refs: Vec<&str> = criteria.iter().map(|c| c.criterion_id.as_str()).collect();
        refs.sort();
        idp_id(
            "verification.contract",
            Json::obj([
                (
                    "criteria",
                    Json::Arr(refs.iter().map(|r| Json::str(*r)).collect()),
                ),
                ("goal", Json::str(goal_semantic_id)),
            ])
            .to_canonical_string()
            .as_bytes(),
        )
    }

    /// The `deliver` precondition (ADR-0109 D3): a `held_out` criterion is
    /// never the payload of `context.artefact.delivered` — refused with
    /// `HeldOutLeak` (and the same predicate is a veto invariant over the
    /// ledger).
    pub fn deliverable(criterion: &AcceptanceCriterion) -> Result<(), ContractError> {
        if criterion.record.visibility == Visibility::HeldOut {
            return Err(ContractError::HeldOutLeak {
                criterion_id: criterion.criterion_id.clone(),
            });
        }
        Ok(())
    }

    /// The `required` non-held-out criteria (the gate's pass table).
    pub fn required_criteria(&self) -> impl Iterator<Item = &AcceptanceCriterion> {
        self.criteria
            .iter()
            .filter(|c| c.record.required && c.record.visibility == Visibility::Visible)
    }
}

/// Contract-level failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    /// A `held_out` criterion was offered for delivery (`HeldOutLeak` — an
    /// error at `deliver`, a refusal at `seal`, a veto invariant over the
    /// ledger).
    HeldOutLeak {
        /// The leaky criterion.
        criterion_id: String,
    },
}

impl std::fmt::Display for ContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContractError::HeldOutLeak { criterion_id } => {
                write!(f, "HeldOutLeak: {criterion_id}")
            }
        }
    }
}

impl std::error::Error for ContractError {}

/// `VerificationSummary` — the kernel-computed gate summary (ADR-0109 D5):
/// `{contract_id, required: {pass, fail, inconclusive, not_run},
/// invariants_tripped[], evidence_head_seq, freshness_ok, held_out_status}`.
#[derive(Debug, Clone, PartialEq)]
pub struct VerificationSummary {
    /// The contract id.
    pub contract_id: String,
    /// `required` criteria that passed.
    pub required_pass: u64,
    /// `required` criteria that failed.
    pub required_fail: u64,
    /// `required` criteria inconclusive.
    pub required_inconclusive: u64,
    /// `required` criteria never run (`VerificationSkipped` when the run
    /// proposes completion — a veto invariant, ADR-0109 D4).
    pub required_not_run: u64,
    /// The tripped invariants.
    pub invariants_tripped: Vec<String>,
    /// The ledger head the evidence was read at.
    pub evidence_head_seq: u64,
    /// Whether freshness held across the required evidence.
    pub freshness_ok: bool,
    /// `run | deferred_to_instrument`.
    pub held_out_status: HeldOutStatus,
}

/// `held_out_status` — whether held-out criteria ran in-band or defer to the
/// instrument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeldOutStatus {
    /// `run` — the held-out checks ran.
    Run,
    /// `deferred_to_instrument` — the instrument owns them.
    DeferredToInstrument,
}

// ── Γ — the intervention table with kernel floors (ADR-0113 D8) ─────────────

/// `DecisionPoint` — where Γ/the gate runs (`stop` vs mid-run placement —
/// F1/F6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DecisionPoint {
    /// `stop` — the completion gate.
    Stop,
    /// `verify` — a mid-run verification placement.
    Verify,
    /// `authorize` — an authorization placement.
    Authorize,
    /// `compact` — a compaction placement.
    Compact,
    /// `delegate` — a delegation placement.
    Delegate,
}

impl DecisionPoint {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DecisionPoint::Stop => "stop",
            DecisionPoint::Verify => "verify",
            DecisionPoint::Authorize => "authorize",
            DecisionPoint::Compact => "compact",
            DecisionPoint::Delegate => "delegate",
        }
    }

    /// Whether the point is the completion gate (`stop` — F1's "where").
    pub fn is_gate(self) -> bool {
        matches!(self, DecisionPoint::Stop)
    }
}

/// `GammaRow` — one Γ table row (`HarnessRule`-shaped MUST-data in the sealed
/// definition): the intervention for `(divergence_class, severity_floor?,
/// effective_risk_class?, decision_point)`.
#[derive(Debug, Clone, PartialEq)]
pub struct GammaRow {
    /// The divergence class the row governs (`None` = the default row).
    pub divergence_class: Option<DivergenceClass>,
    /// The minimum severity the row covers (`None` = any).
    pub severity_floor: Option<SeverityLevel>,
    /// The decision point the row applies at (`None` = any).
    pub decision_point: Option<DecisionPoint>,
    /// The declared intervention.
    pub intervention: Intervention,
    /// The row's `HarnessRule`/`rule_ref` (assumption-debt-bearing for
    /// profile-conditioned rows — AC-R-2.7.2a-10).
    pub rule_ref: String,
    /// The assumption-debt record a profile-conditioned row must carry.
    pub assumption_debt: Option<String>,
    /// Whether the row is profile-conditioned (such rows require
    /// `assumption_debt`).
    pub profile_conditioned: bool,
}

/// `Gamma` — the intervention table (a `HarnessRule` table in the sealed
/// definition; MUST-data).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Gamma {
    /// The declared rows.
    pub rows: Vec<GammaRow>,
}

/// `validate_gamma` failures (a Γ that loosens a floor is refused at
/// `validate` — AC-R-2.7.2a-10).
#[derive(Debug, Clone, PartialEq)]
pub enum GammaError {
    /// A row's intervention is below the kernel floor for its key —
    /// `ScopeCeilingExceeded`-style refusal.
    FloorLoosened {
        /// The offending rule.
        rule_ref: String,
        /// The kernel floor.
        floor: Intervention,
        /// The declared (weaker) intervention.
        declared: Intervention,
    },
    /// A profile-conditioned row without an assumption-debt record.
    ConditionedRowWithoutDebt {
        /// The offending rule.
        rule_ref: String,
    },
}

impl std::fmt::Display for GammaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GammaError::FloorLoosened {
                rule_ref,
                floor,
                declared,
            } => write!(
                f,
                "FloorLoosened: {rule_ref} declares {} below kernel floor {}",
                declared.as_str(),
                floor.as_str()
            ),
            GammaError::ConditionedRowWithoutDebt { rule_ref } => {
                write!(f, "ConditionedRowWithoutDebt: {rule_ref}")
            }
        }
    }
}

impl std::error::Error for GammaError {}

/// The kernel floor for `(divergence_class, decision_point)` (F2–F7 — the
/// MUST-code constants Γ may tighten, never loosen):
///
/// - At the gate (`stop`): any deterministic divergence on the completion
///   claim ⇒ floor `hold` (F2/F3); the `unachievable` honest-failure path is
///   exempt by construction (F5 — an honest failure is never held).
/// - Elsewhere (`verify`/`authorize`/`compact`/`delegate`): floor `annotate`
///   (F6); `escalate`/`stop` outside the gate requires an `irreversible`-class
///   subject effect at `severity = critical` (C2 — R-2.7.2b).
pub fn kernel_floor(
    divergence_class: DivergenceClass,
    decision_point: DecisionPoint,
) -> Intervention {
    if decision_point.is_gate() {
        // F2–F5: a deterministic divergence on the completion claim holds.
        Intervention::Hold
    } else {
        // F6: kernel floor `annotate` elsewhere.
        let _ = divergence_class;
        Intervention::Annotate
    }
}

/// `validate_gamma` — the seal-time check (ADR-0113 D8; AC-R-2.7.2a-10): every
/// row's intervention must be ≥ the kernel floor for its key; every
/// profile-conditioned row must carry an assumption-debt record.
pub fn validate_gamma(gamma: &Gamma) -> Result<(), GammaError> {
    for row in &gamma.rows {
        if row.profile_conditioned && row.assumption_debt.is_none() {
            return Err(GammaError::ConditionedRowWithoutDebt {
                rule_ref: row.rule_ref.clone(),
            });
        }
        // The strictest floor the row could apply under: if the row may fire
        // at `stop` it must satisfy the gate floor; rows pinned to non-gate
        // points satisfy the `annotate` floor.
        let points: &[DecisionPoint] = match row.decision_point {
            Some(p) => match p {
                DecisionPoint::Stop => &[DecisionPoint::Stop],
                _ => &[DecisionPoint::Verify],
            },
            None => &[DecisionPoint::Stop], // an unscoped row may fire at the gate
        };
        for &p in points {
            let floor = kernel_floor(
                row.divergence_class.unwrap_or(DivergenceClass::ContractGap),
                p,
            );
            if !row.intervention.at_least(floor) {
                return Err(GammaError::FloorLoosened {
                    rule_ref: row.rule_ref.clone(),
                    floor,
                    declared: row.intervention,
                });
            }
        }
    }
    Ok(())
}

/// `intervene(record, Γ)` — the declared row wins when it tightens;
/// `PolicyUndefined` ⇒ the kernel floor.
pub fn intervene(
    class: DivergenceClass,
    severity: SeverityLevel,
    decision_point: DecisionPoint,
    gamma: &Gamma,
) -> Intervention {
    let floor = kernel_floor(class, decision_point);
    let mut best = floor;
    for row in &gamma.rows {
        let class_ok = row.divergence_class.is_none_or(|c| c == class);
        let sev_ok = row.severity_floor.is_none_or(|s| severity >= s);
        let point_ok = row.decision_point.is_none_or(|p| p == decision_point);
        if class_ok && sev_ok && point_ok && row.intervention > best {
            best = row.intervention;
        }
    }
    best
}

// ── Gate evaluation (ADR-0113 D1–D5, D9) ────────────────────────────────────

/// `GateFacts` — the deterministic inputs the pure gate fold reads. Every
/// fact is kernel/environment-authoritative by construction (the caller
/// projects them from the ledger); a `parsed`/`judged` record can never
/// appear here (F7).
#[derive(Debug, Clone, PartialEq)]
pub struct GateFacts {
    /// The completion claim's kind (`achieved` | `unachievable` | other —
    /// the gate evaluates completion claims; a non-completion claim is a
    /// caller error).
    pub completion_claim_kind: crate::vocab::ClaimKind,
    /// The completion claim's agreement (a reconciled `unachievable` claim
    /// with `agree`/`unverifiable` is F5's honest failure).
    pub completion_claim_agreement: crate::vocab::Agreement,
    /// The claim's cited evidence refs.
    pub claim_evidence_refs: Vec<String>,
    /// Non-terminal effects still open at the claim (`{effect_id,
    /// detachable}` — F2(a)'s `committed`/`unknown`/`probed(undeterminable)`
    /// and F2(c)'s `detachable` children).
    pub open_effects: Vec<OpenEffect>,
    /// `abandoned` effect ids (F2(b) — success-with-veto).
    pub abandoned_effects: Vec<String>,
    /// The completion claim's `criteria_status` measured by the gate (the
    /// required criteria and their measured state — the contract's pass
    /// table).
    pub required_criteria: Vec<GateCriterion>,
    /// Divergence records already produced on the completion claim's own
    /// `evidence_refs` (F3's second half) — deterministic detectors only;
    /// judged records never reach this list by construction.
    pub claim_evidence_divergences: Vec<DivergenceClass>,
    /// Holds already consumed (`hold_count` — a ledger fact).
    pub holds_consumed: u64,
    /// The `reconciliation.holds` cap (default [`DEFAULT_HOLDS_CAP`]).
    pub holds_cap: u64,
}

/// An open (non-terminal) effect at the completion claim.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenEffect {
    /// The effect id.
    pub effect_id: String,
    /// The non-terminal state tag (`committed` | `unknown` | `probed`).
    pub state: String,
    /// Whether the capability declares `detachable` (F2(c) — annotate only).
    pub detachable: bool,
}

/// One required criterion's gate-side fact.
#[derive(Debug, Clone, PartialEq)]
pub struct GateCriterion {
    /// The criterion ref.
    pub criterion_ref: String,
    /// The measured state.
    pub state: CriterionState,
    /// The bound executable validator refs (F3's `require_validator` action —
    /// empty for a criterion with no bound validator).
    pub bound_validators: Vec<String>,
    /// Whether the criterion declares an `unverifiable_reason` (such a
    /// criterion never causes a hold — F3).
    pub unverifiable_reason: bool,
}

/// The default `reconciliation.holds` cap (F4 — Stage-3 default 3, per-suite
/// override OQ-279; the constant is the schema's declared default).
pub const DEFAULT_HOLDS_CAP: u64 = 3;

/// `GateResult` — `{verdict, hold_count, evidence_refs[]}` (ADR-0113 D1) plus
/// the C0 annotations the run status needs (`exhausted`, `honest_failure`,
/// `success_with_veto`).
#[derive(Debug, Clone, PartialEq)]
pub struct GateResult {
    /// The gate verdict.
    pub verdict: GateVerdict,
    /// The hold count *after* this evaluation (durable, replayable).
    pub hold_count: u64,
    /// The evidence refs the verdict cites.
    pub evidence_refs: Vec<String>,
    /// `hold_count` exceeded the `reconciliation.holds` cap — the run ends
    /// `budget_exhausted{reconciliation.holds}` (stratum
    /// `unreconciled_claims`) or escalates when a principal is reachable
    /// (F4 — the envelope decides; the gate reports).
    pub holds_exhausted: bool,
    /// F5: an `unachievable` claim reconciled `agree`/`unverifiable` — the run
    /// ends an honest failure (`failed` + `failed_honestly` stratum), never
    /// held.
    pub honest_failure: bool,
    /// F2(b): `abandoned` effects exist — `pass` is annotated
    /// success-with-veto under the `inconsistent_durable_state` invariant.
    pub success_with_veto: Option<String>,
}

/// `evaluate_gate` — the pure gate fold (ADR-0113 D1–D5): deterministic over
/// `GateFacts`, replayable, never performs an effect. Floors implemented:
/// - F5 (honest failure passes) — an `unachievable` claim with `agree`/
///   `unverifiable` is `pass` + `honest_failure`.
/// - F2(a) — a non-terminal non-detachable effect at an `achieved` claim ⇒
///   `hold{d6, resolve_effect(id)}`; `detachable` children annotate only.
/// - F2(b) — `abandoned` effects ⇒ `pass` annotated success-with-veto.
/// - F3 — a `required` criterion `unmet`/`unverifiable`/`unrun` with a bound
///   validator ⇒ `hold{d5, require_validator(refs)}`; a declared
///   `unverifiable_reason` criterion never holds; deterministic divergences
///   on the claim's own `evidence_refs` ⇒ `hold`.
/// - F4 — each `hold` consumes one `reconciliation.holds` unit; exhaustion is
///   reported on `holds_exhausted` (the envelope maps it to
///   `budget_exhausted{reconciliation.holds}`/`escalate`).
/// - F7 — the facts are deterministic-only by construction; the verdict can
///   never cite a `judged`/`parsed` record.
pub fn evaluate_gate(facts: &GateFacts) -> GateResult {
    let mut evidence_refs: Vec<String> = facts.claim_evidence_refs.clone();
    let mut divergences: Vec<DivergenceClass> = Vec::new();
    let mut required_actions: Vec<String> = Vec::new();
    let mut success_with_veto = None;

    use crate::vocab::{Agreement, ClaimKind};

    // F5 — honest failure passes.
    if facts.completion_claim_kind == ClaimKind::Unachievable
        && matches!(
            facts.completion_claim_agreement,
            Agreement::Agree | Agreement::Unverifiable
        )
    {
        return GateResult {
            verdict: GateVerdict::Pass,
            hold_count: facts.holds_consumed,
            evidence_refs,
            holds_exhausted: false,
            honest_failure: true,
            success_with_veto: None,
        };
    }

    let is_achieved = facts.completion_claim_kind == ClaimKind::Achieved;

    // F2 — OQ-092: non-terminal effects at an `achieved` claim.
    if is_achieved {
        for e in &facts.open_effects {
            if e.detachable {
                // F2(c) — a `committed` detached child whose capability
                // declares `detachable` ⇒ annotate only.
                continue;
            }
            divergences.push(DivergenceClass::OpenEffectAtCompletion);
            required_actions.push(format!("resolve_effect({})", e.effect_id));
            evidence_refs.push(format!("effect:{}", e.effect_id));
        }
        // F2(b) — `abandoned` ⇒ pass annotated success-with-veto.
        if !facts.abandoned_effects.is_empty() {
            success_with_veto = Some("inconsistent_durable_state".to_string());
        }
    }

    // F3 — contract: unmet/unverifiable/unrun required criteria with a bound
    // validator ⇒ hold{require_validator(refs)}; `unverifiable_reason`
    // criteria never hold.
    for c in &facts.required_criteria {
        if c.unverifiable_reason || c.state.is_met() {
            continue;
        }
        if !c.bound_validators.is_empty() {
            divergences.push(DivergenceClass::ContractGap);
            for v in &c.bound_validators {
                required_actions.push(format!("require_validator({v})"));
            }
            evidence_refs.push(format!("criterion:{}", c.criterion_ref));
        }
        // An unmet required criterion with *no* bound validator is still a
        // contract gap — the gate records the divergence without an action.
        else {
            divergences.push(DivergenceClass::ContractGap);
            evidence_refs.push(format!("criterion:{}", c.criterion_ref));
        }
    }

    // F3's second half — deterministic divergences on the completion claim's
    // own evidence_refs (already-reconciled D2/D3 records).
    divergences.extend(facts.claim_evidence_divergences.iter().copied());

    divergences.sort_by_key(|d| d.code().to_string());
    divergences.dedup();
    required_actions.sort();
    required_actions.dedup();
    evidence_refs.sort();
    evidence_refs.dedup();

    if divergences.is_empty() {
        return GateResult {
            verdict: GateVerdict::Pass,
            hold_count: facts.holds_consumed,
            evidence_refs,
            holds_exhausted: facts.holds_consumed >= facts.holds_cap,
            honest_failure: false,
            success_with_veto,
        };
    }

    // F4 — each hold consumes one `reconciliation.holds` unit.
    let hold_count = facts.holds_consumed + 1;
    GateResult {
        verdict: GateVerdict::Hold {
            divergences,
            required_actions,
        },
        hold_count,
        evidence_refs,
        holds_exhausted: hold_count > facts.holds_cap,
        honest_failure: false,
        success_with_veto: None,
    }
}

/// `CompletionDecision` — the `verification.completion.decided` payload's
/// record half: the run status the gate/envelope resolved to.
#[derive(Debug, Clone, PartialEq)]
pub struct CompletionDecision {
    /// The run id.
    pub run_id: String,
    /// The status (`succeeded` | `failed` | `succeeded_unverified` | …).
    pub status: String,
    /// The `scored` stratum, when one applies (`failed_honestly` |
    /// `unreconciled_claims` — ADR-0161 D5).
    pub stratum: Option<String>,
    /// The gate evaluation's event ref.
    pub gate_ref: String,
    /// The verification summary the decision renders.
    pub summary: VerificationSummary,
}

/// The status mapping of a `GateResult` (the C0 rule the envelope applies):
/// `pass` + `honest_failure` ⇒ `failed`/`failed_honestly`; `hold` +
/// `holds_exhausted` ⇒ `budget_exhausted{reconciliation.holds}`/
/// `unreconciled_claims`; `veto`/`pass` + `success_with_veto` ⇒ success-with-
/// veto; `pass` ⇒ `succeeded` (the `unverifiable` completion policy renders
/// `succeeded_unverified` — the contract's call, not the gate's).
pub fn decision_stratum(result: &GateResult) -> Option<&'static str> {
    if result.honest_failure {
        return Some(STRATUM_FAILED_HONESTLY);
    }
    if result.holds_exhausted && matches!(result.verdict, GateVerdict::Hold { .. }) {
        return Some(STRATUM_UNRECONCILED_CLAIMS);
    }
    None
}

/// The `false_completion` veto check (AC-R-2.7.2a-9's registered invariant):
/// an `achieved` claim contradicted by a deterministic oracle *after* the
/// gate passed is the `false_completion` veto invariant — registered here as
/// the C0 flag (the Stage-3 scorecard computes it).
pub const FALSE_COMPLETION_INVARIANT: &str = VETO_FALSE_COMPLETION;

/// The oracle-failure rendering (a gate can't decide on a failed oracle —
/// `oracle_failure` is an outcome class, ADR-0047 D4).
pub fn oracle_failure_reason(cause: OracleCause) -> &'static str {
    cause.as_str()
}

// ── canonical JSON ──────────────────────────────────────────────────────────

impl GateResult {
    /// The `verification.gate.evaluated` payload half (the event builder in
    /// `events.rs` wraps it with the claim/budget refs).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        let (verdict, divergences, actions) = match &self.verdict {
            GateVerdict::Pass => (Json::str("pass"), Json::Arr(vec![]), Json::Arr(vec![])),
            GateVerdict::Hold {
                divergences,
                required_actions,
            } => (
                Json::str("hold"),
                Json::Arr(divergences.iter().map(|d| Json::str(d.as_str())).collect()),
                Json::Arr(required_actions.iter().map(Json::str).collect()),
            ),
            GateVerdict::Veto { invariant } => (
                Json::str("veto"),
                Json::Arr(vec![Json::str(invariant.clone())]),
                Json::Arr(vec![]),
            ),
        };
        m.insert("verdict".to_string(), verdict);
        m.insert("hold_count".to_string(), Json::Int(self.hold_count as i64));
        m.insert("divergences".to_string(), divergences);
        m.insert("required_actions".to_string(), actions);
        m.insert(
            "evidence_refs".to_string(),
            Json::Arr(self.evidence_refs.iter().map(Json::str).collect()),
        );
        m.insert(
            "holds_exhausted".to_string(),
            Json::Bool(self.holds_exhausted),
        );
        m.insert(
            "honest_failure".to_string(),
            Json::Bool(self.honest_failure),
        );
        Json::Obj(m)
    }
}

/// The detector-class guard (F7's construction half): a record admissible as
/// a *hold/veto cause* must be `detector = deterministic`. Callers filter
/// `claim_evidence_divergences` through this — a judged record can annotate,
/// never cause.
pub fn hold_admissible(detector: Detector) -> bool {
    detector == Detector::Deterministic
}

/// Whether a `Detector`+`EvidenceClass` pair may *cause* a `hold`/`veto`/
/// `escalate`/`stop` (F7 — never solely a `parsed` or `judged` record).
pub fn may_cause_hold(detector: Detector, extracted_by_parsed_or_judged: bool) -> bool {
    detector == Detector::Deterministic && !extracted_by_parsed_or_judged
}

/// The set of `OracleClass`es headline capability may read (re-exported for
/// the gate-side checks).
pub fn headline_oracle(class: crate::vocab::OracleClass) -> bool {
    class.is_c0_headline()
}

/// `BTreeSet` helper for criterion-ref collections.
pub fn criterion_refs(criteria: &[GateCriterion]) -> BTreeSet<String> {
    criteria.iter().map(|c| c.criterion_ref.clone()).collect()
}

// ── TaskContract projection (§5f.2 §3; S3.10 — the seal-time half) ────────

/// The default `ValidatesRecord` a `success_criteria` member projects with
/// (ADR-0109 D2's ratified default: `acceptance`, `completion` phase,
/// `required`, `visible` — a criterion's non-default fields ride the
/// `validates` *record* on the projection input, not the HIR edge).
pub fn default_validates_record() -> ValidatesRecord {
    ValidatesRecord {
        role: CriterionRole::Acceptance,
        phase: VerdictPhase::Completion,
        required: true,
        visibility: Visibility::Visible,
        veto: false,
        evidence_requirements: vec![],
        window: None,
        weight_ppm: None,
    }
}

/// `project_contract` failures — the projection never invents members.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectError {
    /// The named node is absent or not a `Goal`.
    NoGoal {
        /// The ref that failed.
        goal_ref: String,
    },
    /// A `success_criteria` member resolves to a kind that cannot validate
    /// (only `Validator`/`Procedure` refs project).
    BadCriterionKind {
        /// The offending ref.
        criterion_ref: String,
        /// The resolved kind spelling.
        kind: String,
    },
    /// A criterion ref resolves to nothing in the document.
    MissingCriterion {
        /// The unresolved ref.
        criterion_ref: String,
    },
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectError::NoGoal { goal_ref } => write!(f, "no_goal{{{goal_ref}}}"),
            ProjectError::BadCriterionKind { criterion_ref, kind } => {
                write!(f, "bad_criterion_kind{{{criterion_ref}:{kind}}}")
            }
            ProjectError::MissingCriterion { criterion_ref } => {
                write!(f, "missing_criterion{{{criterion_ref}}}")
            }
        }
    }
}

impl std::error::Error for ProjectError {}

/// `project_contract` — the `TaskContract` projection over a sealed
/// document's `Goal` (spec §5f.2 §3; ADR-0109 D1: a typed projection, never
/// an HIR entity, computed at `seal`/`open`, never edited during a run).
/// Each `success_criteria` ref must resolve to a `Validator` or `Procedure`
/// node (a `Procedure` criterion's validator_ref names the procedure — its
/// postcondition rows are the detector); `unverifiable_reason` projects the
/// `unverifiable` completion policy (such a run ends
/// `succeeded_unverified`, never `succeeded`). `held_out` visibility comes
/// from `criterion_visibility` — the caller supplies the sealed
/// `visibility` map (`{criterion_semantic_id → held_out}`) read from the
/// goal's `ext`/`delivers` declarations; absent ⇒ `visible`.
pub fn project_contract(
    doc: &hh_hir::document::HirDocument,
    goal_semantic_id: &str,
    criterion_visibility: &BTreeMap<String, Visibility>,
) -> Result<TaskContract, ProjectError> {
    use hh_hir::records::KindRecord;

    let node = doc
        .node(goal_semantic_id)
        .ok_or_else(|| ProjectError::NoGoal {
            goal_ref: goal_semantic_id.to_string(),
        })?;
    let KindRecord::Goal(goal) = &node.semantic else {
        return Err(ProjectError::NoGoal {
            goal_ref: goal_semantic_id.to_string(),
        });
    };
    let mut criteria = Vec::new();
    for r in &goal.success_criteria {
        let target = doc
            .node(&r.semantic_id)
            .ok_or_else(|| ProjectError::MissingCriterion {
                criterion_ref: r.semantic_id.clone(),
            })?;
        let kind_ok = matches!(
            target.semantic,
            KindRecord::Validator(_) | KindRecord::Procedure(_)
        );
        if !kind_ok {
            return Err(ProjectError::BadCriterionKind {
                criterion_ref: r.semantic_id.clone(),
                kind: target.kind.name().to_string(),
            });
        }
        let pinned = r.to_json();
        let mut record = default_validates_record();
        if criterion_visibility
            .get(&r.semantic_id)
            .is_some_and(|v| *v == Visibility::HeldOut)
        {
            record.visibility = Visibility::HeldOut;
        }
        criteria.push(AcceptanceCriterion {
            criterion_id: r.semantic_id.clone(),
            validator_ref: match &pinned {
                Json::Obj(_) => pinned.to_canonical_string(),
                _ => r.semantic_id.clone(),
            },
            record,
        });
    }
    let policy = match &goal.unverifiable_reason {
        Some(t) => CompletionPolicy::Unverifiable(
            t.content
                .clone()
                .unwrap_or_else(|| t.content_hash.clone()),
        ),
        None => CompletionPolicy::AllRequired,
    };
    let contract_id = TaskContract::compute_contract_id(goal_semantic_id, &criteria);
    Ok(TaskContract {
        contract_id,
        goal_ref: goal_semantic_id.to_string(),
        criteria,
        invariants: vec![],
        completion_policy: policy,
        evidence_kinds_required: vec![],
        budget_ref: goal.budget.semantic_id.clone(),
        sealed: node.version.sealed,
    })
}

/// `summarize` — the `VerificationSummary` the `completion.decided` payload
/// renders (ADR-0110 D7): the measured pass/fail/inconclusive/not_run
/// counts over the contract's *required* criteria, the tripped invariants,
/// the evidence head, and the held-out status (`deferred_to_instrument`
/// when held-out criteria exist — the scorer boundary owns them, the run
/// never sees them).
pub fn summarize(
    contract: &TaskContract,
    criterion_states: &BTreeMap<String, CriterionState>,
    tripped_invariants: Vec<String>,
    evidence_head_seq: u64,
) -> VerificationSummary {
    let mut pass = 0u64;
    let mut fail = 0u64;
    let mut inconclusive = 0u64;
    let mut not_run = 0u64;
    for c in contract.required_criteria() {
        match criterion_states.get(&c.criterion_id) {
            Some(CriterionState::Met) => pass += 1,
            Some(CriterionState::Unmet) => fail += 1,
            Some(CriterionState::Unverifiable) => inconclusive += 1,
            Some(CriterionState::Unrun) | None => not_run += 1,
        }
    }
    VerificationSummary {
        contract_id: contract.contract_id.clone(),
        required_pass: pass,
        required_fail: fail,
        required_inconclusive: inconclusive,
        required_not_run: not_run,
        invariants_tripped: tripped_invariants,
        evidence_head_seq,
        freshness_ok: true,
        held_out_status: if contract
            .criteria
            .iter()
            .any(|c| c.record.visibility == Visibility::HeldOut)
        {
            HeldOutStatus::DeferredToInstrument
        } else {
            HeldOutStatus::Run
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::ClaimKind;

    fn criterion(state: CriterionState, validators: &[&str]) -> GateCriterion {
        GateCriterion {
            criterion_ref: "crit:1".into(),
            state,
            bound_validators: validators.iter().map(|s| s.to_string()).collect(),
            unverifiable_reason: false,
        }
    }

    fn facts() -> GateFacts {
        GateFacts {
            completion_claim_kind: ClaimKind::Achieved,
            completion_claim_agreement: crate::vocab::Agreement::Agree,
            claim_evidence_refs: vec![],
            open_effects: vec![],
            abandoned_effects: vec![],
            required_criteria: vec![criterion(CriterionState::Met, &[])],
            claim_evidence_divergences: vec![],
            holds_consumed: 0,
            holds_cap: DEFAULT_HOLDS_CAP,
        }
    }

    #[test]
    fn clean_achieved_claim_passes() {
        let r = evaluate_gate(&facts());
        assert_eq!(r.verdict, GateVerdict::Pass);
        assert_eq!(r.hold_count, 0);
        assert!(!r.honest_failure);
    }

    #[test]
    fn f2_open_effect_at_achieved_holds() {
        let mut f = facts();
        f.open_effects.push(OpenEffect {
            effect_id: "e:9".into(),
            state: "unknown".into(),
            detachable: false,
        });
        let r = evaluate_gate(&f);
        match &r.verdict {
            GateVerdict::Hold {
                divergences,
                required_actions,
            } => {
                assert!(divergences.contains(&DivergenceClass::OpenEffectAtCompletion));
                assert_eq!(required_actions, &["resolve_effect(e:9)".to_string()]);
            }
            v => panic!("expected hold, got {v:?}"),
        }
        assert_eq!(r.hold_count, 1); // durable, replayable (F4)
    }

    #[test]
    fn f2_detachable_children_annotate_only() {
        let mut f = facts();
        f.open_effects.push(OpenEffect {
            effect_id: "e:9".into(),
            state: "committed".into(),
            detachable: true,
        });
        let r = evaluate_gate(&f);
        assert_eq!(r.verdict, GateVerdict::Pass);
    }

    #[test]
    fn f2_abandoned_is_success_with_veto() {
        let mut f = facts();
        f.abandoned_effects.push("e:7".into());
        let r = evaluate_gate(&f);
        assert_eq!(r.verdict, GateVerdict::Pass);
        assert_eq!(
            r.success_with_veto.as_deref(),
            Some("inconsistent_durable_state")
        );
    }

    #[test]
    fn f3_unmet_required_criterion_holds_with_require_validator() {
        let mut f = facts();
        f.required_criteria = vec![criterion(CriterionState::Unmet, &["val:1"])];
        let r = evaluate_gate(&f);
        match &r.verdict {
            GateVerdict::Hold {
                divergences,
                required_actions,
            } => {
                assert!(divergences.contains(&DivergenceClass::ContractGap));
                assert_eq!(required_actions, &["require_validator(val:1)".to_string()]);
            }
            v => panic!("expected hold, got {v:?}"),
        }
    }

    #[test]
    fn f3_unverifiable_reason_criterion_never_holds() {
        let mut f = facts();
        f.required_criteria = vec![GateCriterion {
            criterion_ref: "crit:u".into(),
            state: CriterionState::Unverifiable,
            bound_validators: vec![],
            unverifiable_reason: true,
        }];
        let r = evaluate_gate(&f);
        assert_eq!(r.verdict, GateVerdict::Pass);
    }

    #[test]
    fn f3_divergence_on_claims_own_evidence_holds() {
        let mut f = facts();
        f.claim_evidence_divergences
            .push(DivergenceClass::PhantomEffect);
        let r = evaluate_gate(&f);
        assert!(matches!(r.verdict, GateVerdict::Hold { .. }));
    }

    #[test]
    fn f4_holds_consume_budget_and_exhaust() {
        let mut f = facts();
        f.open_effects.push(OpenEffect {
            effect_id: "e:1".into(),
            state: "committed".into(),
            detachable: false,
        });
        f.holds_consumed = 2;
        let r = evaluate_gate(&f);
        assert_eq!(r.hold_count, 3);
        assert!(!r.holds_exhausted);
        f.holds_consumed = 3;
        let r = evaluate_gate(&f);
        assert_eq!(r.hold_count, 4);
        assert!(r.holds_exhausted);
        assert_eq!(decision_stratum(&r), Some(STRATUM_UNRECONCILED_CLAIMS));
    }

    #[test]
    fn f5_unachievable_is_honest_failure_never_held() {
        let mut f = facts();
        f.completion_claim_kind = ClaimKind::Unachievable;
        f.completion_claim_agreement = crate::vocab::Agreement::Unverifiable;
        // even with a contract gap present, the honest-failure path passes
        f.required_criteria = vec![criterion(CriterionState::Unmet, &["v:1"])];
        let r = evaluate_gate(&f);
        assert_eq!(r.verdict, GateVerdict::Pass);
        assert!(r.honest_failure);
        assert_eq!(decision_stratum(&r), Some(STRATUM_FAILED_HONESTLY));
    }

    #[test]
    fn f7_judged_records_never_cause_holds() {
        assert!(!may_cause_hold(Detector::Judged, false));
        assert!(!may_cause_hold(Detector::Deterministic, true)); // parsed/judged record
        assert!(may_cause_hold(Detector::Deterministic, false));
    }

    #[test]
    fn gamma_loosening_is_refused() {
        // AC-R-2.7.2a-10: a Γ row below the kernel floor fails validate.
        let g = Gamma {
            rows: vec![GammaRow {
                divergence_class: Some(DivergenceClass::ContractGap),
                severity_floor: None,
                decision_point: Some(DecisionPoint::Stop),
                intervention: Intervention::Annotate, // below hold
                rule_ref: "rule:weak".into(),
                assumption_debt: None,
                profile_conditioned: false,
            }],
        };
        assert!(matches!(
            validate_gamma(&g),
            Err(GammaError::FloorLoosened { .. })
        ));
        // A tightening row passes.
        let g = Gamma {
            rows: vec![GammaRow {
                divergence_class: Some(DivergenceClass::ContractGap),
                severity_floor: None,
                decision_point: Some(DecisionPoint::Stop),
                intervention: Intervention::Stop,
                rule_ref: "rule:strong".into(),
                assumption_debt: None,
                profile_conditioned: false,
            }],
        };
        validate_gamma(&g).unwrap();
    }

    #[test]
    fn gamma_unscoped_rows_must_satisfy_the_gate_floor() {
        // An unscoped row may fire at `stop` — it must be ≥ hold.
        let g = Gamma {
            rows: vec![GammaRow {
                divergence_class: None,
                severity_floor: None,
                decision_point: None,
                intervention: Intervention::FeedBack, // below hold
                rule_ref: "rule:unscoped".into(),
                assumption_debt: None,
                profile_conditioned: false,
            }],
        };
        assert!(matches!(
            validate_gamma(&g),
            Err(GammaError::FloorLoosened { .. })
        ));
        let g = Gamma {
            rows: vec![GammaRow {
                divergence_class: None,
                severity_floor: None,
                decision_point: None,
                intervention: Intervention::Hold,
                rule_ref: "rule:floor".into(),
                assumption_debt: None,
                profile_conditioned: false,
            }],
        };
        validate_gamma(&g).unwrap();
    }

    #[test]
    fn conditioned_rows_require_debt_records() {
        let g = Gamma {
            rows: vec![GammaRow {
                divergence_class: None,
                severity_floor: None,
                decision_point: Some(DecisionPoint::Verify),
                intervention: Intervention::FeedBack,
                rule_ref: "rule:cond".into(),
                assumption_debt: None,
                profile_conditioned: true,
            }],
        };
        assert!(matches!(
            validate_gamma(&g),
            Err(GammaError::ConditionedRowWithoutDebt { .. })
        ));
    }

    #[test]
    fn intervene_tightens_never_loosens_and_defaults_to_floor() {
        let g = Gamma {
            rows: vec![GammaRow {
                divergence_class: Some(DivergenceClass::ContractGap),
                severity_floor: None,
                decision_point: Some(DecisionPoint::Stop),
                intervention: Intervention::Stop,
                rule_ref: "r".into(),
                assumption_debt: None,
                profile_conditioned: false,
            }],
        };
        // Tightening applies.
        assert_eq!(
            intervene(
                DivergenceClass::ContractGap,
                SeverityLevel::High,
                DecisionPoint::Stop,
                &g
            ),
            Intervention::Stop
        );
        // A weaker row is ignored — the floor stands (PolicyUndefined ⇒ floor).
        assert_eq!(
            intervene(
                DivergenceClass::PhantomEffect,
                SeverityLevel::High,
                DecisionPoint::Stop,
                &g
            ),
            Intervention::Hold
        );
        // Off-gate ⇒ annotate floor.
        assert_eq!(
            intervene(
                DivergenceClass::ContractGap,
                SeverityLevel::High,
                DecisionPoint::Verify,
                &g
            ),
            Intervention::Annotate
        );
    }

    #[test]
    fn held_out_criteria_are_never_deliverable() {
        // AC-R-2.7.1's deliver precondition — HeldOutLeak.
        let c = AcceptanceCriterion {
            criterion_id: "crit:h".into(),
            validator_ref: "sha256:v".into(),
            record: ValidatesRecord {
                role: CriterionRole::Acceptance,
                phase: VerdictPhase::Completion,
                required: true,
                visibility: Visibility::HeldOut,
                veto: true,
                evidence_requirements: vec![],
                window: None,
                weight_ppm: None,
            },
        };
        assert!(matches!(
            TaskContract::deliverable(&c),
            Err(ContractError::HeldOutLeak { .. })
        ));
    }

    #[test]
    fn contract_id_is_deterministic_over_sorted_refs() {
        let mk = |id: &str| AcceptanceCriterion {
            criterion_id: id.to_string(),
            validator_ref: "sha256:v".into(),
            record: ValidatesRecord {
                role: CriterionRole::Acceptance,
                phase: VerdictPhase::Completion,
                required: true,
                visibility: Visibility::Visible,
                veto: false,
                evidence_requirements: vec![],
                window: None,
                weight_ppm: None,
            },
        };
        let a = TaskContract::compute_contract_id("goal:g", &[mk("c:1"), mk("c:2")]);
        let b = TaskContract::compute_contract_id("goal:g", &[mk("c:2"), mk("c:1")]);
        assert_eq!(a, b);
        let c = TaskContract::compute_contract_id("goal:g", &[mk("c:1"), mk("c:3")]);
        assert_ne!(a, c);
    }

    #[test]
    fn gate_result_json_is_canonical() {
        let mut f = facts();
        f.open_effects.push(OpenEffect {
            effect_id: "e:1".into(),
            state: "committed".into(),
            detachable: false,
        });
        let j = evaluate_gate(&f).to_json();
        let s = j.to_canonical_string();
        assert!(s.contains("\"verdict\":\"hold\""));
        assert!(s.contains("\"hold_count\":1"));
    }
}
