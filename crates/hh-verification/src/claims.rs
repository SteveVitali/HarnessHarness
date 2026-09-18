//! `claims` — the claim-ledger substrate (spec §5f.2 §3; ADR-0112, ADR-0114 D3):
//! `Claim`, `AuthoritativeHandle`, `ReconciliationRecord`, `SeverityRecord`,
//! `ReconciliationNotice`, the `bind` kind table and the pure `ledger_only`
//! reconcile fold. A claim is an `Observation{source = model_claim}` — never
//! evidence; every extracted claim carries `authority = delegate` and
//! `evidence_class = claimed`, and `validate` refuses `authority > delegate`
//! with `AuthorityExceedsOrigin` (CC2).

use std::collections::BTreeMap;

use hh_provenance::authority::AuthorityClass;
use hh_provenance::record::ProvenanceRecord;
use hh_wire::Json;

use crate::vocab::{
    Agreement, ChargedTo, ClaimKind, Detector, DivergenceClass, EvidenceClass, ExtractedBy,
    HandleKind, ReconcileMode, SeverityLevel, SourceRegister, SubjectRef,
};

// ── Claim ────────────────────────────────────────────────────────────────────

/// `CriterionState` — the per-criterion status a completion claim asserts
/// (AC-R-2.7.2a-1's `criteria_status`, aligned to `Goal.success_criteria`
/// indices — never an ordinal alone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CriterionState {
    /// `met` — the claim asserts the criterion is met.
    Met,
    /// `unmet` — the claim asserts the criterion failed.
    Unmet,
    /// `unverifiable` — the claim asserts the criterion cannot be checked.
    Unverifiable,
    /// `unrun` — no check ran.
    Unrun,
}

impl CriterionState {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CriterionState::Met => "met",
            CriterionState::Unmet => "unmet",
            CriterionState::Unverifiable => "unverifiable",
            CriterionState::Unrun => "unrun",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<CriterionState> {
        [
            CriterionState::Met,
            CriterionState::Unmet,
            CriterionState::Unverifiable,
            CriterionState::Unrun,
        ]
        .into_iter()
        .find(|c| c.as_str() == s)
    }

    /// Whether the state is an affirmative completion state.
    pub fn is_met(self) -> bool {
        matches!(self, CriterionState::Met)
    }
}

/// One `criteria_status[]` entry (a completion claim's per-criterion assertion).
#[derive(Debug, Clone, PartialEq)]
pub struct CriterionStatus {
    /// The criterion ref (`semantic_id` or `validates` ref — never an ordinal).
    pub criterion_ref: String,
    /// The asserted state.
    pub status: CriterionState,
}

impl CriterionStatus {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("criterion_ref", Json::str(self.criterion_ref.clone())),
            ("status", Json::str(self.status.as_str())),
        ])
    }
}

/// `Claim` — a typed claim extracted from an AgentProcess output (ADR-0112 D1).
/// A claim is *not* evidence: it is an `Observation{source = model_claim}` at
/// `authority = delegate`, `evidence_class = claimed` — fixed, not a field.
#[derive(Debug, Clone, PartialEq)]
pub struct Claim {
    /// The claim id.
    pub claim_id: String,
    /// The run the claim belongs to.
    pub run_id: String,
    /// The model call that produced it.
    pub model_call_id: String,
    /// The ledger seq the claim is *about*.
    pub at_seq: u64,
    /// The claim kind.
    pub kind: ClaimKind,
    /// The claim subject.
    pub subject: SubjectRef,
    /// The claimed predicate (`exists`, `applied`, `passed`, `is_done`, …).
    pub predicate: String,
    /// The asserted value (closed-schema JSON).
    pub asserted: Json,
    /// The evidence refs the claim cites (`EventRef | ContentAddress`).
    pub evidence_refs: Vec<String>,
    /// How the claim was extracted.
    pub extracted_by: ExtractedBy,
    /// The extraction confidence in ppm (≤ the `extracted_by` ceiling).
    pub extraction_confidence_ppm: u64,
    /// The completion claim's per-criterion status table (empty for
    /// non-completion claims).
    pub criteria_status: Vec<CriterionStatus>,
    /// Who produced the claim — `origin ∈ {model, participant, subagent}`,
    /// `authority = delegate` (enforced by [`Claim::validate`]).
    pub provenance: ProvenanceRecord,
}

/// `Claim` validation failures (typed — never a warning, ADR-0033 D7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimError {
    /// `authority > delegate` — a claim may never assert authority above its
    /// origin (`AuthorityExceedsOrigin`, the extract/append refusal).
    AuthorityExceedsOrigin {
        /// The ceiling `delegate` confers.
        ceiling: AuthorityClass,
        /// The authority the record carried.
        claimed: AuthorityClass,
    },
    /// `extraction_confidence` exceeds the `extracted_by` ceiling
    /// (`parsed`/`judged` claims are never 1.0).
    ConfidenceExceedsCeiling {
        /// The extraction channel.
        extracted_by: String,
        /// The ceiling (ppm).
        ceiling_ppm: u64,
        /// The claimed confidence (ppm).
        claimed_ppm: u64,
    },
    /// A `criteria_status` entry carried an unaligned/empty `criterion_ref`.
    UnalignedCriterionRef,
}

impl std::fmt::Display for ClaimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClaimError::AuthorityExceedsOrigin { ceiling, claimed } => write!(
                f,
                "AuthorityExceedsOrigin: claim authority {claimed:?} exceeds {ceiling:?}"
            ),
            ClaimError::ConfidenceExceedsCeiling {
                extracted_by,
                ceiling_ppm,
                claimed_ppm,
            } => write!(
                f,
                "MalformedClaim: {extracted_by} confidence {claimed_ppm} exceeds {ceiling_ppm} ppm"
            ),
            ClaimError::UnalignedCriterionRef => {
                write!(f, "MalformedClaim: empty criterion_ref in criteria_status")
            }
        }
    }
}

impl std::error::Error for ClaimError {}

impl Claim {
    /// `evidence_class` is fixed: every claim is `claimed` (ADR-0115 D3).
    pub fn evidence_class(&self) -> EvidenceClass {
        EvidenceClass::Claimed
    }

    /// The extract/append contract: `authority ≤ delegate` (a claim at any
    /// higher authority is refused — AC-R-2.7.2a-1) and the extraction
    /// confidence respects the channel ceiling.
    pub fn validate(&self) -> Result<(), ClaimError> {
        if self.provenance.authority > AuthorityClass::Delegate {
            return Err(ClaimError::AuthorityExceedsOrigin {
                ceiling: AuthorityClass::Delegate,
                claimed: self.provenance.authority,
            });
        }
        let ceiling = self.extracted_by.confidence_ceiling_ppm();
        if self.extraction_confidence_ppm > ceiling {
            return Err(ClaimError::ConfidenceExceedsCeiling {
                extracted_by: self.extracted_by.kind_tag().to_string(),
                ceiling_ppm: ceiling,
                claimed_ppm: self.extraction_confidence_ppm,
            });
        }
        if self
            .criteria_status
            .iter()
            .any(|c| c.criterion_ref.is_empty())
        {
            return Err(ClaimError::UnalignedCriterionRef);
        }
        Ok(())
    }
}

// ── AuthoritativeHandle ──────────────────────────────────────────────────────

/// `HandleValue` — the closed-schema value a bound handle carries (what the
/// deterministic detectors read — never the claim's own text).
#[derive(Debug, Clone, PartialEq)]
pub enum HandleValue {
    /// An `effect_state`/`action.effect.*` fact.
    EffectState {
        /// The effect id.
        effect_id: String,
        /// Whether the effect reached a terminal event.
        terminal: bool,
        /// Whether the terminal outcome was a refusal.
        refused: bool,
        /// The canonical outcome tag (`ok`, `error`, `refused`, `unknown`,
        /// `committed`, `probed`, `abandoned`, `detached`).
        outcome: String,
    },
    /// A `validator_verdict` fact — the verdict's affirmative read.
    Verdict {
        /// Whether the verdict affirms (`pass`/`true`/`N`).
        affirmative: bool,
    },
    /// A `task_contract`/`criterion` fact.
    Criterion {
        /// The criterion's measured state.
        status: CriterionState,
    },
    /// Any other closed-schema handle value (opaque to the C0 detectors —
    /// equality comparison only).
    Value(Json),
}

impl HandleValue {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        match self {
            HandleValue::EffectState {
                effect_id,
                terminal,
                refused,
                outcome,
            } => Json::obj([
                ("kind", Json::str("effect_state")),
                ("effect_id", Json::str(effect_id.clone())),
                ("terminal", Json::Bool(*terminal)),
                ("refused", Json::Bool(*refused)),
                ("outcome", Json::str(outcome.clone())),
            ]),
            HandleValue::Verdict { affirmative } => Json::obj([
                ("kind", Json::str("verdict")),
                ("affirmative", Json::Bool(*affirmative)),
            ]),
            HandleValue::Criterion { status } => Json::obj([
                ("kind", Json::str("criterion")),
                ("status", Json::str(status.as_str())),
            ]),
            HandleValue::Value(v) => v.clone(),
        }
    }
}

/// `AuthoritativeHandle` — a binding to an authoritative record (ADR-0112 D3).
/// Handles are records at `authority ≥ environment`; model text is never a
/// handle.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthoritativeHandle {
    /// The handle kind.
    pub kind: HandleKind,
    /// The bound record's ref (`EventRef` / `VersionedRef` / `ContentAddress`).
    pub handle_ref: String,
    /// The closed-schema value the reconciler reads, when determinable.
    pub value: Option<HandleValue>,
    /// The handle's authority — stamped by the kernel from the record's
    /// provenance, never read from the handle's own content (CC2).
    pub authority: AuthorityClass,
    /// The seq the handle fact was produced at.
    pub produced_at_seq: u64,
}

/// `AuthoritativeHandle` validation failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandleError {
    /// A closed-schema handle below `environment` (`Unauthoritative` — model
    /// text is never authoritative evidence).
    Unauthoritative {
        /// The authority the handle carried.
        authority: AuthorityClass,
    },
}

impl std::fmt::Display for HandleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HandleError::Unauthoritative { authority } => write!(
                f,
                "Unauthoritative: handle authority {authority:?} below environment"
            ),
        }
    }
}

impl std::error::Error for HandleError {}

impl AuthoritativeHandle {
    /// The handle contract: closed-schema handle values require the
    /// `environment` class (ADR-0112 D3; OQ-102 census: keep). `delegate`
    /// orders *above* `environment` in the command lattice (§8.1 #3) but is
    /// model-claimed text — never authoritative evidence — so it is refused
    /// explicitly rather than by the ordering alone.
    pub fn validate(&self) -> Result<(), HandleError> {
        if self.authority < AuthorityClass::Environment
            || self.authority == AuthorityClass::Delegate
        {
            return Err(HandleError::Unauthoritative {
                authority: self.authority,
            });
        }
        Ok(())
    }
}

// ── bind (ADR-0112 D3) ───────────────────────────────────────────────────────

/// The `bind` kind table — the handle kinds a `ClaimKind` binds to
/// (`observed → context.observation.recorded`/`action.world_state.*`;
/// `effected → action.effect.*`; `verified → verification.validator.verdict`;
/// `achieved → Goal.success_criteria` validators + output contract;
/// `pending →` non-terminal effects/`detached_effect_ids[]`;
/// `progress →` the prior progress-artifact version). `unachievable` and
/// `assumption` bind to nothing — they reconcile `unverifiable`, never
/// `agree` (they are also never divergences by themselves — F5).
pub fn binding_kinds(kind: ClaimKind) -> &'static [HandleKind] {
    match kind {
        ClaimKind::Observed => &[HandleKind::LedgerEvent, HandleKind::WorldState],
        ClaimKind::Effected => &[HandleKind::EffectState, HandleKind::LedgerEvent],
        ClaimKind::Verified => &[HandleKind::ValidatorVerdict],
        ClaimKind::Achieved => &[HandleKind::TaskContract, HandleKind::ValidatorVerdict],
        ClaimKind::Pending => &[HandleKind::EffectState],
        ClaimKind::Progress => &[HandleKind::ProgressArtifactVersion],
        ClaimKind::Unachievable | ClaimKind::Assumption => &[],
    }
}

// ── ReconciliationRecord (ADR-0112 D4) ───────────────────────────────────────

/// `ReconciliationRecord` — the four-valued agreement record
/// (`origin = kernel(reconciler)` for a deterministic detector — the caller
/// stamps `provenance`; `model(judge)` at `delegate` for judged — C2).
#[derive(Debug, Clone, PartialEq)]
pub struct ReconciliationRecord {
    /// The record id.
    pub record_id: String,
    /// The reconciled claim.
    pub claim_id: String,
    /// The four-valued agreement.
    pub agreement: Agreement,
    /// The severity (the execution-alignment register, ADR-0114 D3).
    pub severity: SeverityLevel,
    /// The detector class that produced the record.
    pub detector: Detector,
    /// The detector's pinned ref (the reconciler `version_id`).
    pub detector_ref: String,
    /// The detector confidence in ppm.
    pub confidence_ppm: u64,
    /// The handle records the verdict cites — a `diverge` cites ≥ 1.
    pub evidence_refs: Vec<String>,
    /// The probe effect ids (`probe` mode only — C2; empty at C0).
    pub probe_effect_ids: Vec<String>,
    /// The seq the record was produced at.
    pub reconciled_at_seq: u64,
    /// The reconcile mode.
    pub mode: ReconcileMode,
    /// Who the reconciliation was charged to.
    pub charged_to: ChargedTo,
    /// The record's provenance.
    pub provenance: ProvenanceRecord,
}

/// `ReconciliationRecord` validation failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileError {
    /// A `diverge` cited no handle record — every divergence must cite ≥ 1
    /// authoritative record, never the claim's own text.
    DivergenceWithoutEvidence,
    /// `probe` mode produced no `probe_effect_ids` — a probe-mode record
    /// without probes is malformed at C0 (probes are C2).
    ProbeModeWithoutProbes,
}

impl std::fmt::Display for ReconcileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReconcileError::DivergenceWithoutEvidence => {
                write!(f, "diverge must cite ≥ 1 handle record")
            }
            ReconcileError::ProbeModeWithoutProbes => {
                write!(f, "probe-mode record without probe_effect_ids")
            }
        }
    }
}

impl std::error::Error for ReconcileError {}

impl ReconciliationRecord {
    /// The record's own contract (a `diverge` always cites a handle).
    pub fn validate(&self) -> Result<(), ReconcileError> {
        if matches!(self.agreement, Agreement::Diverge(_)) && self.evidence_refs.is_empty() {
            return Err(ReconcileError::DivergenceWithoutEvidence);
        }
        if self.mode == ReconcileMode::Probe && self.probe_effect_ids.is_empty() {
            return Err(ReconcileError::ProbeModeWithoutProbes);
        }
        Ok(())
    }
}

/// The execution-alignment severity register (ADR-0114 D3): `critical` = false
/// completion with irreversible external effects claimed; `high` = D2/D3/D5 on
/// the completion claim; `medium` = mid-run D2/D3 and deterministic D9;
/// `low` = D4/D7/D8; `info` = the first D10 hit. `on_completion_claim` is the
/// gate-context flag.
pub fn severity_for(class: DivergenceClass, on_completion_claim: bool) -> SeverityLevel {
    match class {
        DivergenceClass::ContractGap | DivergenceClass::OpenEffectAtCompletion => {
            SeverityLevel::High
        }
        DivergenceClass::PhantomEffect | DivergenceClass::UnverifiedVerification => {
            if on_completion_claim {
                SeverityLevel::High
            } else {
                SeverityLevel::Medium
            }
        }
        DivergenceClass::StaleBelief
        | DivergenceClass::CensoredEvidence
        | DivergenceClass::ProgressRegression => SeverityLevel::Low,
        DivergenceClass::EvidenceInversion => SeverityLevel::Medium,
        DivergenceClass::PhantomObservation => SeverityLevel::Medium,
        DivergenceClass::NoProgressLoop => SeverityLevel::Info,
    }
}

/// `SeverityRecord` — the shared OQ-125 shape (`{source_register, level,
/// rule_ref}`); per-register ownership keeps execution-alignment severities
/// distinct from security/effect severities.
#[derive(Debug, Clone, PartialEq)]
pub struct SeverityRecord {
    /// The owning register.
    pub source_register: SourceRegister,
    /// The level.
    pub level: SeverityLevel,
    /// The rule that produced the level.
    pub rule_ref: String,
}

/// `ReconciliationNotice` — a deterministic projection of ledger facts
/// delivered as a `kernel_notice` D1 candidate at `kernel` authority,
/// deduplicated by `(class, subject)` per turn (F6; ADR-0113 D6).
#[derive(Debug, Clone, PartialEq)]
pub struct ReconciliationNotice {
    /// The reconciliation record the notice projects.
    pub record_ref: String,
    /// The divergence class.
    pub class: DivergenceClass,
    /// The claim subject.
    pub subject: SubjectRef,
    /// The expected value (a `HandleValueRef` — the handle's own value ref).
    pub expected: String,
    /// The observed value (the claimed value's ref).
    pub observed: String,
    /// Suggested required actions.
    pub suggested: Vec<String>,
}

/// The pure `ledger_only` reconcile fold — pure in `(claim, handles ≤
/// until_seq, detector version)` (ADR-0112 D4). The caller supplies the bound
/// handles (the resolution of [`binding_kinds`] over the ledger is the
/// caller's); this fold never performs effects and never mutates the claim.
///
/// C0 deterministic detectors (the D2/D3/D5/D6 ledger-only subset):
/// - `effected` claim contradicted by an `effect_state`/`ledger_event` handle ⇒
///   `diverge{phantom_effect}` (D2).
/// - `verified` claim contradicted by a `validator_verdict` handle ⇒
///   `diverge{unverified_verification}` (D3).
/// - `achieved` claim with a required `criterion` handle `unmet`/`unverifiable`/
///   `unrun` ⇒ `diverge{contract_gap}` (D5); with a non-terminal `effect_state`
///   handle ⇒ `diverge{open_effect_at_completion}` (D6).
/// - A handle produced after the claim's `at_seq` that contradicts ⇒ `stale`.
/// - No handles, or no determinable handle value ⇒ `unverifiable` (never
///   `agree`). A determinable contradiction outside the C0 detector set is
///   `unverifiable` — the C2 classes (D1/D4/D7–D10) are never claimed here.
pub fn reconcile_ledger_only(
    claim: &Claim,
    handles: &[AuthoritativeHandle],
    detector_ref: &str,
    reconciled_at_seq: u64,
    provenance: ProvenanceRecord,
) -> ReconciliationRecord {
    let record_id = format!("recon:{}", claim.claim_id);
    let mut record = ReconciliationRecord {
        record_id,
        claim_id: claim.claim_id.clone(),
        agreement: Agreement::Unverifiable,
        severity: SeverityLevel::Info,
        detector: Detector::Deterministic,
        detector_ref: detector_ref.to_string(),
        confidence_ppm: 1_000_000,
        evidence_refs: Vec::new(),
        probe_effect_ids: Vec::new(),
        reconciled_at_seq,
        mode: ReconcileMode::LedgerOnly,
        charged_to: ChargedTo::Subject,
        provenance,
    };

    if handles.is_empty() {
        return record; // unbindable ⇒ unverifiable, never agree
    }

    let on_completion = matches!(claim.kind, ClaimKind::Achieved | ClaimKind::Unachievable)
        || matches!(claim.subject, SubjectRef::Run | SubjectRef::Criterion(_));

    // Partition into stale (post-claim) and contemporaneous handles.
    let mut stale_contradiction = false;
    let mut divergence: Option<(DivergenceClass, String)> = None;
    let mut any_determinable = false;
    let mut any_match = false;

    for h in handles {
        let Some(value) = &h.value else {
            continue; // handle value not determinable — does not decide
        };
        any_determinable = true;
        let contradicts = claim_contradicted(claim, value);
        let diverge_class = c0_divergence_class(claim, value, contradicts);
        if contradicts && h.produced_at_seq > claim.at_seq {
            stale_contradiction = true;
            record.evidence_refs.push(h.handle_ref.clone());
        } else if let (true, Some(class)) = (contradicts, diverge_class) {
            if divergence.is_none() {
                divergence = Some((class, h.handle_ref.clone()));
            }
            record.evidence_refs.push(h.handle_ref.clone());
        } else if !contradicts {
            any_match = true;
            record.evidence_refs.push(h.handle_ref.clone());
        }
    }

    if let Some((class, _)) = divergence {
        record.agreement = Agreement::Diverge(class);
        record.severity = severity_for(class, on_completion);
        record.evidence_refs.sort();
        record.evidence_refs.dedup();
    } else if stale_contradiction {
        record.agreement = Agreement::Stale;
        record.severity = SeverityLevel::Low;
    } else if any_match {
        record.agreement = Agreement::Agree;
        record.severity = SeverityLevel::Info;
    } else if !any_determinable {
        record.agreement = Agreement::Unverifiable;
        record.severity = SeverityLevel::Low;
        record.evidence_refs.clear();
    } else {
        // A determinable contradiction with no C0 detector class — honest
        // `unverifiable` citing the handles (never `agree`, never a C2 class).
        record.agreement = Agreement::Unverifiable;
        record.severity = SeverityLevel::Low;
    }

    record
}

/// Whether the handle value contradicts the claim's assertion (closed-schema
/// comparisons only — the claim's own text is never the comparator).
fn claim_contradicted(claim: &Claim, value: &HandleValue) -> bool {
    match (claim.kind, value) {
        (
            ClaimKind::Effected,
            HandleValue::EffectState {
                refused,
                outcome,
                terminal,
                ..
            },
        ) => {
            // Claimed success while the record shows a refusal, an error, or a
            // non-terminal state — a D2 contradiction. `unknown`/`committed`
            // contradict an `applied`-style assertion only for achieved claims;
            // for a plain effected claim a non-terminal effect is *not yet* a
            // contradiction (the effect lifecycle owns resolution).
            if *refused {
                return true;
            }
            *terminal && (outcome == "error" || outcome == "abandoned")
        }
        (ClaimKind::Verified, HandleValue::Verdict { affirmative }) => !affirmative,
        (ClaimKind::Achieved, HandleValue::Criterion { status }) => !status.is_met(),
        (ClaimKind::Achieved, HandleValue::EffectState { terminal, .. }) => !terminal,
        (ClaimKind::Pending, HandleValue::EffectState { terminal, .. }) => *terminal,
        (_, HandleValue::Value(v)) => *v != claim.asserted,
        _ => false,
    }
}

/// The C0 detector class a contradiction resolves to, when one is declared —
/// `None` keeps the record `unverifiable` rather than inventing a C2 verdict.
fn c0_divergence_class(
    claim: &Claim,
    value: &HandleValue,
    contradicts: bool,
) -> Option<DivergenceClass> {
    if !contradicts {
        return None;
    }
    match (claim.kind, value) {
        (ClaimKind::Effected, HandleValue::EffectState { .. })
        | (ClaimKind::Effected, HandleValue::Value(_)) => Some(DivergenceClass::PhantomEffect),
        (ClaimKind::Verified, HandleValue::Verdict { .. }) => {
            Some(DivergenceClass::UnverifiedVerification)
        }
        (ClaimKind::Achieved, HandleValue::Criterion { status }) if !status.is_met() => {
            Some(DivergenceClass::ContractGap)
        }
        (ClaimKind::Achieved, HandleValue::EffectState { terminal, .. }) if !terminal => {
            Some(DivergenceClass::OpenEffectAtCompletion)
        }
        _ => None,
    }
}

// ── canonical JSON ──────────────────────────────────────────────────────────

impl Claim {
    /// The `Claim` record's canonical JSON (the `verification.claim.recorded`
    /// `claim` member per ADR-0112 D6 — refs are rendered, never inlined
    /// blobs).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("claim_id".to_string(), Json::str(self.claim_id.clone()));
        m.insert("run_id".to_string(), Json::str(self.run_id.clone()));
        m.insert(
            "model_call_id".to_string(),
            Json::str(self.model_call_id.clone()),
        );
        m.insert("at_seq".to_string(), Json::Int(self.at_seq as i64));
        m.insert("kind".to_string(), Json::str(self.kind.as_str()));
        m.insert("subject".to_string(), self.subject.to_json());
        m.insert("predicate".to_string(), Json::str(self.predicate.clone()));
        m.insert("asserted".to_string(), self.asserted.clone());
        m.insert(
            "evidence_refs".to_string(),
            Json::Arr(self.evidence_refs.iter().map(Json::str).collect()),
        );
        m.insert(
            "extracted_by".to_string(),
            Json::obj([
                ("kind", Json::str(self.extracted_by.kind_tag())),
                (
                    "ref",
                    match &self.extracted_by {
                        ExtractedBy::Structured(r)
                        | ExtractedBy::Parsed(r)
                        | ExtractedBy::Judged(r) => Json::str(r.clone()),
                    },
                ),
            ]),
        );
        m.insert(
            "extraction_confidence_ppm".to_string(),
            Json::Int(self.extraction_confidence_ppm as i64),
        );
        m.insert(
            "criteria_status".to_string(),
            Json::Arr(self.criteria_status.iter().map(|c| c.to_json()).collect()),
        );
        m.insert(
            "evidence_class".to_string(),
            Json::str(self.evidence_class().as_str()),
        );
        Json::Obj(m)
    }
}

// ── extract ─────────────────────────────────────────────────────────────────

/// `ExtractError` — the `extract` refusal set (ADR-0112 D1/D2; surfaced to
/// the model as a Contract/Format observation — never a panic).
#[derive(Debug, Clone, PartialEq)]
pub enum ExtractError {
    /// The response carried no claim surface (`NoClaimSurface`).
    NoClaimSurface,
    /// A claim field violated its closed schema (`MalformedClaim`).
    MalformedClaim(String),
    /// A produced claim failed [`Claim::validate`] — the extract/append
    /// refusal (`AuthorityExceedsOrigin`, confidence ceiling, unaligned
    /// criterion ref) rides the same typed error.
    Invalid(ClaimError),
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtractError::NoClaimSurface => write!(f, "NoClaimSurface"),
            ExtractError::MalformedClaim(d) => write!(f, "MalformedClaim: {d}"),
            ExtractError::Invalid(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ExtractError {}

/// `extract(model_call_id, response_view, claim_surface, …) → [Claim]` — the
/// C0/Stage-1 structured channel (spec §5f.2 §2; ADR-0112 D1/D2). Reads the
/// compiled claim-surface field `claim_surface` of `response_view` — the
/// completion capability's `finish{criteria_status[], artifacts_claimed[],
/// open_items[]}` record. Structured claims extract at confidence 1.0; the
/// `parsed`/`judged` channels are later stages and never produced here.
///
/// Emitted claims: one completion claim (`kind ∈ {achieved, unachievable}`,
/// `subject = run`, the `criteria_status` table carried) plus one `effected`
/// claim per `artifacts_claimed[]` entry and one `pending` claim per
/// `open_items[]` entry — every claim at `authority = delegate`,
/// `evidence_class = claimed`, [`Claim::validate`]-checked.
///
/// Refusals: `NoClaimSurface` when the surface field is absent/not an object;
/// `MalformedClaim` on a closed-schema violation (unknown `completion`/
/// `status` spelling, empty `criterion_ref`, non-string ref); `Invalid` when a
/// produced claim fails `validate` (a `provenance` above `delegate` is refused
/// here — AC-R-2.7.2a-1's `AuthorityExceedsOrigin`).
pub fn extract(
    model_call_id: &str,
    response_view: &Json,
    claim_surface: &str,
    run_id: &str,
    at_seq: u64,
    provenance: ProvenanceRecord,
) -> Result<Vec<Claim>, ExtractError> {
    let finish = response_view
        .get(claim_surface)
        .cloned()
        .unwrap_or(Json::Null);
    let fields = match &finish {
        Json::Obj(m) if !m.is_empty() => m,
        _ => return Err(ExtractError::NoClaimSurface),
    };
    // The completion kind — absent ⇒ `achieved` (the model invoked the
    // completion capability; that invocation *is* the completion claim).
    let kind = match fields.get("completion") {
        None | Some(Json::Null) => ClaimKind::Achieved,
        Some(Json::Str(s)) => match ClaimKind::parse(s) {
            Some(k @ (ClaimKind::Achieved | ClaimKind::Unachievable)) => k,
            Some(other) => {
                return Err(ExtractError::MalformedClaim(format!(
                    "completion kind {other:?} ∉ {{achieved, unachievable}}"
                )));
            }
            None => {
                return Err(ExtractError::MalformedClaim(format!(
                    "completion spelling {s:?} unknown"
                )));
            }
        },
        Some(_) => {
            return Err(ExtractError::MalformedClaim(
                "completion is not a string".into(),
            ));
        }
    };
    // `criteria_status[]` — `{criterion_ref, status ∈ {met, unmet,
    // unverifiable, unrun}, evidence_refs[]?}` per entry.
    let mut criteria_status = Vec::new();
    let mut evidence_refs = Vec::new();
    if let Some(v) = fields.get("criteria_status") {
        let arr = match v {
            Json::Null => return Err(ExtractError::NoClaimSurface),
            Json::Arr(a) => a,
            _ => {
                return Err(ExtractError::MalformedClaim(
                    "criteria_status is not an array".into(),
                ));
            }
        };
        for entry in arr {
            let obj = match entry {
                Json::Obj(o) => o,
                _ => {
                    return Err(ExtractError::MalformedClaim(
                        "criteria_status entry is not an object".into(),
                    ));
                }
            };
            let criterion_ref = match obj.get("criterion_ref") {
                Some(Json::Str(s)) if !s.is_empty() => s.clone(),
                _ => {
                    return Err(ExtractError::MalformedClaim(
                        "criteria_status entry missing/empty criterion_ref".into(),
                    ));
                }
            };
            let status = match obj.get("status") {
                Some(Json::Str(s)) => CriterionState::parse(s).ok_or_else(|| {
                    ExtractError::MalformedClaim(format!("criterion status {s:?} unknown"))
                })?,
                _ => {
                    return Err(ExtractError::MalformedClaim(
                        "criteria_status entry missing status".into(),
                    ));
                }
            };
            if let Some(Json::Arr(refs)) = obj.get("evidence_refs") {
                for r in refs {
                    match r {
                        Json::Str(s) => evidence_refs.push(s.clone()),
                        _ => {
                            return Err(ExtractError::MalformedClaim(
                                "evidence_refs entry is not a string".into(),
                            ));
                        }
                    }
                }
            }
            criteria_status.push(CriterionStatus {
                criterion_ref,
                status,
            });
        }
    }
    // `artifacts_claimed[]` — one `effected` claim per ref (the model claims
    // it produced the artifact; `subject = artifact{…}`).
    let mut artifact_claims = Vec::new();
    if let Some(v) = fields.get("artifacts_claimed") {
        match v {
            Json::Null | Json::Arr(_) => {}
            _ => {
                return Err(ExtractError::MalformedClaim(
                    "artifacts_claimed is not an array".into(),
                ));
            }
        }
        if let Json::Arr(items) = v {
            for item in items {
                match item {
                    Json::Str(s) if !s.is_empty() => artifact_claims.push(s.clone()),
                    _ => {
                        return Err(ExtractError::MalformedClaim(
                            "artifacts_claimed entry is not a non-empty string".into(),
                        ));
                    }
                }
            }
        }
    }
    // `open_items[]` — one `pending` claim per item id.
    let mut open_items = Vec::new();
    if let Some(v) = fields.get("open_items") {
        match v {
            Json::Null | Json::Arr(_) => {}
            _ => {
                return Err(ExtractError::MalformedClaim(
                    "open_items is not an array".into(),
                ));
            }
        }
        if let Json::Arr(items) = v {
            for item in items {
                match item {
                    Json::Str(s) if !s.is_empty() => open_items.push(s.clone()),
                    _ => {
                        return Err(ExtractError::MalformedClaim(
                            "open_items entry is not a non-empty string".into(),
                        ));
                    }
                }
            }
        }
    }
    let mut claims = Vec::with_capacity(1 + artifact_claims.len() + open_items.len());
    let extracted_by = ExtractedBy::Structured(claim_surface.to_string());
    let mut next = 0u64;
    let mut push = |claim: &mut Claim| {
        next += 1;
        claim.claim_id = format!("{model_call_id}:claim:{next}");
        claim.validate().map_err(ExtractError::Invalid)
    };
    let mut completion = Claim {
        claim_id: String::new(),
        run_id: run_id.to_string(),
        model_call_id: model_call_id.to_string(),
        at_seq,
        kind,
        subject: SubjectRef::Run,
        predicate: "is_done".into(),
        asserted: finish.clone(),
        evidence_refs,
        extracted_by: extracted_by.clone(),
        extraction_confidence_ppm: 1_000_000,
        criteria_status,
        provenance: provenance.clone(),
    };
    push(&mut completion)?;
    claims.push(completion);
    for artifact in artifact_claims {
        let mut c = Claim {
            claim_id: String::new(),
            run_id: run_id.to_string(),
            model_call_id: model_call_id.to_string(),
            at_seq,
            kind: ClaimKind::Effected,
            subject: SubjectRef::Artifact(artifact.clone()),
            predicate: "exists".into(),
            asserted: Json::str(artifact),
            evidence_refs: vec![],
            extracted_by: extracted_by.clone(),
            extraction_confidence_ppm: 1_000_000,
            criteria_status: vec![],
            provenance: provenance.clone(),
        };
        push(&mut c)?;
        claims.push(c);
    }
    for item in open_items {
        let mut c = Claim {
            claim_id: String::new(),
            run_id: run_id.to_string(),
            model_call_id: model_call_id.to_string(),
            at_seq,
            kind: ClaimKind::Pending,
            subject: SubjectRef::ProgressItem(item.clone()),
            predicate: "open".into(),
            asserted: Json::str(item),
            evidence_refs: vec![],
            extracted_by: extracted_by.clone(),
            extraction_confidence_ppm: 1_000_000,
            criteria_status: vec![],
            provenance: provenance.clone(),
        };
        push(&mut c)?;
        claims.push(c);
    }
    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_provenance::authority::PersistenceScope;
    use hh_provenance::origin::Origin;

    fn delegate_prov() -> ProvenanceRecord {
        ProvenanceRecord::minted(
            Origin::model("model/x", "snapshot:1", "call:1"),
            PersistenceScope::Run,
            7,
        )
    }

    fn claim(kind: ClaimKind, subject: SubjectRef, asserted: Json) -> Claim {
        Claim {
            claim_id: "c1".into(),
            run_id: "run:1".into(),
            model_call_id: "call:1".into(),
            at_seq: 10,
            kind,
            subject,
            predicate: "applied".into(),
            asserted,
            evidence_refs: vec![],
            extracted_by: ExtractedBy::Structured("surface.claims".into()),
            extraction_confidence_ppm: 1_000_000,
            criteria_status: vec![],
            provenance: delegate_prov(),
        }
    }

    fn handle(kind: HandleKind, value: HandleValue, seq: u64) -> AuthoritativeHandle {
        AuthoritativeHandle {
            kind,
            handle_ref: format!("evt:{kind:?}"),
            value: Some(value),
            authority: AuthorityClass::Kernel,
            produced_at_seq: seq,
        }
    }

    #[test]
    fn claim_above_delegate_is_refused() {
        let mut c = claim(ClaimKind::Achieved, SubjectRef::Run, Json::str("done"));
        c.provenance.authority = AuthorityClass::Principal;
        assert_eq!(
            c.validate(),
            Err(ClaimError::AuthorityExceedsOrigin {
                ceiling: AuthorityClass::Delegate,
                claimed: AuthorityClass::Principal,
            })
        );
    }

    #[test]
    fn parsed_confidence_is_capped() {
        let mut c = claim(ClaimKind::Progress, SubjectRef::Run, Json::str("p"));
        c.extracted_by = ExtractedBy::Parsed("grammar:claims".into());
        c.extraction_confidence_ppm = 900_000;
        assert!(matches!(
            c.validate(),
            Err(ClaimError::ConfidenceExceedsCeiling { .. })
        ));
        c.extraction_confidence_ppm = 800_000;
        c.validate().unwrap();
    }

    #[test]
    fn claims_have_fixed_evidence_class() {
        let c = claim(ClaimKind::Achieved, SubjectRef::Run, Json::Null);
        assert_eq!(c.evidence_class(), EvidenceClass::Claimed);
        assert_eq!(c.provenance.authority, AuthorityClass::Delegate);
    }

    #[test]
    fn unbindable_claim_is_unverifiable_never_agree() {
        let c = claim(ClaimKind::Assumption, SubjectRef::Run, Json::str("x"));
        assert!(binding_kinds(c.kind).is_empty());
        let r = reconcile_ledger_only(&c, &[], "hir/kernel/reconcile:0", 20, delegate_prov());
        assert_eq!(r.agreement, Agreement::Unverifiable);
        r.validate().unwrap();
    }

    #[test]
    fn d2_phantom_effect_fires_on_refused_effect() {
        let c = claim(
            ClaimKind::Effected,
            SubjectRef::Effect("e:1".into()),
            Json::str("applied"),
        );
        let h = handle(
            HandleKind::EffectState,
            HandleValue::EffectState {
                effect_id: "e:1".into(),
                terminal: true,
                refused: true,
                outcome: "refused".into(),
            },
            5,
        );
        let r = reconcile_ledger_only(&c, &[h], "hir/kernel/reconcile:0", 20, delegate_prov());
        assert_eq!(
            r.agreement,
            Agreement::Diverge(DivergenceClass::PhantomEffect)
        );
        assert_eq!(r.detector, Detector::Deterministic);
        assert!(!r.evidence_refs.is_empty());
        r.validate().unwrap();
    }

    #[test]
    fn d3_unverified_verification_fires_on_failed_verdict() {
        let c = claim(
            ClaimKind::Verified,
            SubjectRef::ValidatorTarget("tests".into()),
            Json::Bool(true),
        );
        let h = handle(
            HandleKind::ValidatorVerdict,
            HandleValue::Verdict { affirmative: false },
            5,
        );
        let r = reconcile_ledger_only(&c, &[h], "hir/kernel/reconcile:0", 20, delegate_prov());
        assert_eq!(
            r.agreement,
            Agreement::Diverge(DivergenceClass::UnverifiedVerification)
        );
        assert_eq!(r.severity, SeverityLevel::Medium); // mid-run (subject isn't Run/Criterion)
    }

    #[test]
    fn d5_contract_gap_and_d6_open_effect_on_completion_claim() {
        let c = claim(ClaimKind::Achieved, SubjectRef::Run, Json::str("done"));
        let gap = handle(
            HandleKind::TaskContract,
            HandleValue::Criterion {
                status: CriterionState::Unmet,
            },
            5,
        );
        let r = reconcile_ledger_only(&c, &[gap], "hir/kernel/reconcile:0", 20, delegate_prov());
        assert_eq!(
            r.agreement,
            Agreement::Diverge(DivergenceClass::ContractGap)
        );
        assert_eq!(r.severity, SeverityLevel::High);

        let open = handle(
            HandleKind::EffectState,
            HandleValue::EffectState {
                effect_id: "e:9".into(),
                terminal: false,
                refused: false,
                outcome: "committed".into(),
            },
            5,
        );
        let r = reconcile_ledger_only(&c, &[open], "hir/kernel/reconcile:0", 20, delegate_prov());
        assert_eq!(
            r.agreement,
            Agreement::Diverge(DivergenceClass::OpenEffectAtCompletion)
        );
    }

    #[test]
    fn agree_and_stale_paths() {
        let c = claim(
            ClaimKind::Verified,
            SubjectRef::ValidatorTarget("tests".into()),
            Json::Bool(true),
        );
        let ok = handle(
            HandleKind::ValidatorVerdict,
            HandleValue::Verdict { affirmative: true },
            5,
        );
        let r = reconcile_ledger_only(&c, &[ok], "hir/kernel/reconcile:0", 20, delegate_prov());
        assert_eq!(r.agreement, Agreement::Agree);

        // A post-claim contradiction is stale, not diverge.
        let later = handle(
            HandleKind::ValidatorVerdict,
            HandleValue::Verdict { affirmative: false },
            15,
        );
        let c2 = claim(
            ClaimKind::Verified,
            SubjectRef::ValidatorTarget("tests".into()),
            Json::Bool(true),
        );
        let r = reconcile_ledger_only(&c2, &[later], "hir/kernel/reconcile:0", 20, delegate_prov());
        assert_eq!(r.agreement, Agreement::Stale);
    }

    #[test]
    fn diverge_without_evidence_is_malformed() {
        let mut r = reconcile_ledger_only(
            &claim(ClaimKind::Assumption, SubjectRef::Run, Json::Null),
            &[],
            "d",
            1,
            delegate_prov(),
        );
        r.agreement = Agreement::Diverge(DivergenceClass::PhantomEffect);
        assert_eq!(r.validate(), Err(ReconcileError::DivergenceWithoutEvidence));
    }

    #[test]
    fn handle_below_environment_is_unauthoritative() {
        let h = AuthoritativeHandle {
            kind: HandleKind::LedgerEvent,
            handle_ref: "evt:1".into(),
            value: None,
            authority: AuthorityClass::Delegate, // model text is never a handle
            produced_at_seq: 1,
        };
        assert_eq!(
            h.validate(),
            Err(HandleError::Unauthoritative {
                authority: AuthorityClass::Delegate,
            })
        );
    }

    #[test]
    fn ledger_only_fold_is_pure_and_deterministic() {
        // R1: same (claim, handles, detector version) ⇒ identical record.
        let c = claim(
            ClaimKind::Effected,
            SubjectRef::Effect("e:1".into()),
            Json::str("applied"),
        );
        let h = handle(
            HandleKind::EffectState,
            HandleValue::EffectState {
                effect_id: "e:1".into(),
                terminal: true,
                refused: true,
                outcome: "refused".into(),
            },
            5,
        );
        let a = reconcile_ledger_only(&c, std::slice::from_ref(&h), "d:0", 20, delegate_prov());
        let b = reconcile_ledger_only(&c, &[h], "d:0", 20, delegate_prov());
        assert_eq!(a, b);
    }
    // ── extract (AC-R-2.7.2a-1) ─────────────────────────────────────────────

    fn finish_view(finish: Json) -> Json {
        Json::obj([("finish", finish)])
    }

    #[test]
    fn extract_completed_stop_emits_achieved_with_aligned_criteria() {
        let finish = Json::obj([
            (
                "criteria_status",
                Json::Arr(vec![
                    Json::obj([
                        ("criterion_ref", Json::str("c:build-green")),
                        ("status", Json::str("met")),
                        ("evidence_refs", Json::Arr(vec![Json::str("sha256:ev1")])),
                    ]),
                    Json::obj([
                        ("criterion_ref", Json::str("c:tests-pass")),
                        ("status", Json::str("met")),
                    ]),
                ]),
            ),
            (
                "artifacts_claimed",
                Json::Arr(vec![Json::str("sha256:patch-1")]),
            ),
            ("open_items", Json::Arr(vec![Json::str("item:lint")])),
        ]);
        let claims = extract(
            "mc-7",
            &finish_view(finish),
            "finish",
            "run:1",
            42,
            delegate_prov(),
        )
        .unwrap();
        assert_eq!(claims.len(), 3);
        let c = &claims[0];
        // AC-R-2.7.2a-1: kind ∈ {achieved, unachievable}, aligned criteria,
        // delegate provenance, structured extraction at confidence 1.0.
        assert_eq!(c.kind, ClaimKind::Achieved);
        assert_eq!(c.subject, SubjectRef::Run);
        assert_eq!(c.criteria_status.len(), 2);
        assert_eq!(c.criteria_status[0].status, CriterionState::Met);
        assert_eq!(c.evidence_refs, vec!["sha256:ev1".to_string()]);
        assert_eq!(
            c.provenance.authority,
            hh_provenance::authority::AuthorityClass::Delegate
        );
        assert_eq!(c.extraction_confidence_ppm, 1_000_000);
        assert!(matches!(c.extracted_by, ExtractedBy::Structured(_)));
        assert_eq!(c.claim_id, "mc-7:claim:1");
        // The artifact claim is `effected` on `artifact{…}`; the open item is
        // `pending` on `progress_item{…}`.
        assert_eq!(claims[1].kind, ClaimKind::Effected);
        assert_eq!(
            claims[1].subject,
            SubjectRef::Artifact("sha256:patch-1".into())
        );
        assert_eq!(claims[2].kind, ClaimKind::Pending);
        assert_eq!(
            claims[2].subject,
            SubjectRef::ProgressItem("item:lint".into())
        );
    }

    #[test]
    fn extract_unachievable_is_the_honest_failure_channel() {
        let finish = Json::obj([("completion", Json::str("unachievable"))]);
        let claims = extract(
            "mc-1",
            &finish_view(finish),
            "finish",
            "run:1",
            1,
            delegate_prov(),
        )
        .unwrap();
        assert_eq!(claims[0].kind, ClaimKind::Unachievable);
    }

    #[test]
    fn extract_no_claim_surface() {
        assert_eq!(
            extract("mc-1", &Json::Null, "finish", "run:1", 1, delegate_prov()),
            Err(ExtractError::NoClaimSurface)
        );
        // An empty object carries no claim surface either.
        assert_eq!(
            extract(
                "mc-1",
                &finish_view(Json::obj([])),
                "finish",
                "run:1",
                1,
                delegate_prov()
            ),
            Err(ExtractError::NoClaimSurface)
        );
    }

    #[test]
    fn extract_malformed_claim_refusals() {
        // Unknown completion kind.
        let bad_kind = finish_view(Json::obj([("completion", Json::str("verified"))]));
        assert!(matches!(
            extract("mc-1", &bad_kind, "finish", "run:1", 1, delegate_prov()),
            Err(ExtractError::MalformedClaim(_))
        ));
        // Unknown criterion status.
        let bad_status = finish_view(Json::obj([(
            "criteria_status",
            Json::Arr(vec![Json::obj([
                ("criterion_ref", Json::str("c:1")),
                ("status", Json::str("green")),
            ])]),
        )]));
        assert!(matches!(
            extract("mc-1", &bad_status, "finish", "run:1", 1, delegate_prov()),
            Err(ExtractError::MalformedClaim(_))
        ));
        // Empty criterion_ref.
        let empty_ref = finish_view(Json::obj([(
            "criteria_status",
            Json::Arr(vec![Json::obj([
                ("criterion_ref", Json::str("")),
                ("status", Json::str("met")),
            ])]),
        )]));
        assert!(matches!(
            extract("mc-1", &empty_ref, "finish", "run:1", 1, delegate_prov()),
            Err(ExtractError::MalformedClaim(_))
        ));
    }

    #[test]
    fn extract_refuses_authority_above_delegate() {
        // AC-R-2.7.2a-1's `AuthorityExceedsOrigin`: a claim may never carry
        // authority above `delegate` — extract refuses at the boundary.
        let principal = ProvenanceRecord::minted(
            Origin::human("author:1", hh_provenance::origin::HumanRole::Principal),
            PersistenceScope::Run,
            7,
        );
        let finish = finish_view(Json::obj([("completion", Json::str("achieved"))]));
        let err = extract("mc-1", &finish, "finish", "run:1", 1, principal);
        assert!(matches!(
            err,
            Err(ExtractError::Invalid(
                ClaimError::AuthorityExceedsOrigin { .. }
            ))
        ));
    }

    #[test]
    fn extract_is_deterministic() {
        // R1-shaped purity: same inputs ⇒ identical claims (ids derive from
        // model_call_id, never clocks).
        let finish = finish_view(Json::obj([(
            "criteria_status",
            Json::Arr(vec![Json::obj([
                ("criterion_ref", Json::str("c:1")),
                ("status", Json::str("met")),
            ])]),
        )]));
        let a = extract("mc-9", &finish, "finish", "run:1", 5, delegate_prov()).unwrap();
        let b = extract("mc-9", &finish, "finish", "run:1", 5, delegate_prov()).unwrap();
        assert_eq!(a, b);
    }
}
