//! `reconciler` — the `execution_alignment` C2 variant (R-2.7.2b; spec
//! §5f.3; ADR-0112 D4/D5, ADR-0113 D6/D8). The component class:
//!
//! - `ReconcilerDeclaration` — the runtime declaration
//!   `{classes_detected, detector_classes, probe_capabilities,
//!   requires_observability, applies_to, cost_profile, enabled,
//!   conditioned_on, assumption_debt}` (ADR-0241 shape — the
//!   `conditioned_on ⇒ assumption_debt` discipline mirrors the critic
//!   declaration).
//! - `reconcile_c2` — the C2 fold: the C0 `ledger_only` detectors plus the
//!   deterministic arms of D1 `phantom_observation`, D4 `stale_belief`,
//!   D7 `censored_evidence`, D8 `progress_regression`, D9
//!   `evidence_inversion`, D10 `no_progress_loop`. A claim can emit several
//!   records (the D7+D10 pair AC-R-2.7.2b-2 names).
//! - `reconcile_judged` — the `detector = judged` arm: delegate-authoritative
//!   records carrying `calibration_ref` + `independence_summary`, never
//!   admissible as gate holds (F7).
//! - `reconcile_probe` + `ProbePort` — `probe` mode: a probe is an
//!   `action.probe` planned step dispatched through the *ordinary* effect
//!   lifecycle (`read_only ∧ closed_world` capabilities only); the probe's
//!   result enters the reconciliation evidence bundle; `probe_denied`
//!   exhausts to `unverifiable`, never to agree.
//! - `feed_back_notices` — the `kernel_notice` projection: per-turn
//!   `(class, subject)` dedup, refusal-naming suggestions.
//! - `cap_outside_gate` — mid-run interventions are `annotate|feed_back`;
//!   `escalate|stop` survive only at `severity = critical` on an
//!   `irreversible`-external subject (the §05a-owned kill path).
//! - `gamma_from_rules` + `disable_gamma_recipe` — the Γ table *is*
//!   `HarnessRule` definition data (a γ row is a `HarnessRule` whose
//!   trigger carries `kind = "intervention"`); the "disable Γ for model M"
//!   recipe emits a `removal_test` the Lab executes as
//!   `interventions_experiment` (AC-R-2.7.2b-5).
//! - `evaluate_gate_hosted` — floor-9: the hosted participant's gate keeps
//!   the D5 shape over `end_state` handles (declared end-state assertions
//!   vs the environment's measured end-state reads — C1 hosting).
//!
//! Everything here is pure: the caller projects `ReconcileContext` facts
//! from the ledger; this module never performs effects, never mutates the
//! claim, and never widens authority (CC2/CC8 — additive records only).

use std::collections::{BTreeMap, BTreeSet};

use hh_ontology::participant::Observability;
use hh_provenance::record::ProvenanceRecord;
use hh_wire::Json;

use crate::claims::{
    reconcile_ledger_only, severity_for, AuthoritativeHandle, Claim, ReconciliationNotice,
    ReconciliationRecord,
};
use crate::critics::ProbeCapability;
use crate::gate::{DecisionPoint, Gamma, GammaError, GammaRow, GateResult};
use crate::vocab::{
    Agreement, ChargedTo, ClaimKind, Detector, DivergenceClass, GateVerdict, Intervention,
    ReconcileMode, SeverityLevel, SubjectRef,
};

/// The D10 no-progress window default (`reconciler.no_progress_k`, OQ-279 —
/// the suite's `k`; two identical repeated actions with no state change is
/// the smallest loop the detector can certify).
pub const DEFAULT_NO_PROGRESS_K: u64 = 2;

// ── ReconcilerDeclaration (ADR-0113 §5f.3; R-2.7.2b) ─────────────────────────

/// `ReconcilerDeclaration` — the `execution_alignment` runtime declaration.
/// A run that declares the variant binds exactly one declaration; `enabled =
/// false` is the ablation switch (the variant collapses to the C0
/// `ledger_only` fold unchanged — AC-R-2.7.2b-4's removal surface).
#[derive(Debug, Clone, PartialEq)]
pub struct ReconcilerDeclaration {
    /// The reconciler's pinned ref (the `detector_ref` its records name).
    pub reconciler_ref: String,
    /// `classes_detected ⊆ {d1,…,d10}` — the divergence classes the variant
    /// may emit.
    pub classes_detected: BTreeSet<DivergenceClass>,
    /// `detector_classes ⊆ {deterministic, judged}` — which detectors the
    /// declaration admits (`human` verdicts enter through the principal
    /// channel, never this declaration).
    pub detector_classes: BTreeSet<Detector>,
    /// `probe_capabilities[]` — the declared probe capabilities (each
    /// `read_only ∧ closed_world` — checked at `declare`).
    pub probe_capabilities: Vec<ProbeCapability>,
    /// `requires_observability ⊆ {events, model_io, end_state, ledger}` —
    /// the observability the declaration needs (the `n/a` floor for
    /// AC-R-2.7.3-12's per-class table).
    pub requires_observability: BTreeSet<Observability>,
    /// `applies_to ⊆ claim kinds` — the claim kinds the variant reconciles
    /// (empty = all).
    pub applies_to: BTreeSet<ClaimKind>,
    /// The probe budget share cap (`probe_budget_share_cap`, ppm — the
    /// share of the subject budget probes may consume).
    pub probe_budget_share_cap_ppm: Option<u64>,
    /// `conditioned_on` — a `ProfileRef` the declaration is conditioned on
    /// (such a declaration *requires* `assumption_debt` — refused otherwise).
    pub conditioned_on: Option<String>,
    /// The assumption-debt record a profile-conditioned declaration must
    /// carry.
    pub assumption_debt: Option<String>,
    /// The judged extractor/detector ref the `judged` arm names.
    pub judge_ref: Option<String>,
    /// The `CalibrationRecord` the judged detectors ran under (mandatory
    /// when `detector_classes ∋ judged`).
    pub calibration_ref: Option<String>,
    /// The independence dimensions the judged detectors relied on.
    pub independence_summary: Option<String>,
    /// `no_progress_k` — the D10 repetition window (default
    /// [`DEFAULT_NO_PROGRESS_K`]; OQ-279's per-suite `k`).
    pub no_progress_k: u64,
    /// The ablation switch — `false` collapses the variant to C0
    /// (`reconcile_ledger_only` output unchanged).
    pub enabled: bool,
}

/// `declare_reconciler` failures (a declaration that admits a non-read-only
/// or open-world probe, a profile-conditioned declaration without debt, or
/// a judged detector without calibration is refused at `declare` —
/// AC-R-2.7.2b-1/AC-R-2.7.3-2).
#[derive(Debug, Clone, PartialEq)]
pub enum ReconcilerError {
    /// `classes_detected` empty — a reconciler detecting nothing is
    /// malformed.
    EmptyClassSet,
    /// A probe capability not declared `read_only`.
    ProbeNotReadOnly {
        /// The offending capability.
        capability_ref: String,
    },
    /// A probe capability not `closed_world`.
    ProbeNotClosedWorld {
        /// The offending capability.
        capability_ref: String,
    },
    /// `conditioned_on` set without `assumption_debt`.
    ConditionedWithoutDebt,
    /// `detector_classes ∋ judged` without a `calibration_ref` +
    /// `judge_ref` — a judged arm with no calibration is malformed.
    JudgedWithoutCalibration,
}

impl std::fmt::Display for ReconcilerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReconcilerError::EmptyClassSet => {
                write!(f, "EmptyClassSet: classes_detected is empty")
            }
            ReconcilerError::ProbeNotReadOnly { capability_ref } => write!(
                f,
                "ProbeNotReadOnly: {capability_ref} not declared read_only"
            ),
            ReconcilerError::ProbeNotClosedWorld { capability_ref } => {
                write!(f, "ProbeNotClosedWorld: {capability_ref} not closed_world")
            }
            ReconcilerError::ConditionedWithoutDebt => {
                write!(
                    f,
                    "ConditionedWithoutDebt: conditioned_on without assumption_debt"
                )
            }
            ReconcilerError::JudgedWithoutCalibration => {
                write!(
                    f,
                    "JudgedWithoutCalibration: judged detector without calibration_ref"
                )
            }
        }
    }
}

impl std::error::Error for ReconcilerError {}

/// `declare_reconciler(decl) → ()` — the seal-time check (AC-R-2.7.2b-1):
/// every probe capability `read_only ∧ closed_world`; `conditioned_on`
/// requires `assumption_debt`; a `judged` detector class requires
/// `calibration_ref` + `judge_ref`.
pub fn declare_reconciler(decl: &ReconcilerDeclaration) -> Result<(), ReconcilerError> {
    if decl.classes_detected.is_empty() {
        return Err(ReconcilerError::EmptyClassSet);
    }
    for cap in &decl.probe_capabilities {
        if !cap.read_only {
            return Err(ReconcilerError::ProbeNotReadOnly {
                capability_ref: cap.capability_ref.clone(),
            });
        }
        if !cap.closed_world {
            return Err(ReconcilerError::ProbeNotClosedWorld {
                capability_ref: cap.capability_ref.clone(),
            });
        }
    }
    if decl.conditioned_on.is_some() && decl.assumption_debt.is_none() {
        return Err(ReconcilerError::ConditionedWithoutDebt);
    }
    if decl.detector_classes.contains(&Detector::Judged)
        && (decl.calibration_ref.is_none() || decl.judge_ref.is_none())
    {
        return Err(ReconcilerError::JudgedWithoutCalibration);
    }
    Ok(())
}

// ── ReconcileContext — the caller-projected C2 facts ─────────────────────────

/// `Watermark` — the last-changed fact a `world_state`/`context` record
/// carries for a subject (D4's comparator).
#[derive(Debug, Clone, PartialEq)]
pub struct Watermark {
    /// The seq the subject last measurably changed at.
    pub changed_at_seq: u64,
    /// The change record's ref (the D4 record's evidence).
    pub change_ref: String,
}

/// `ProgressFact` — the measured state of a progress item (D8's
/// comparator — projected from `context.progress.*` and bound validator
/// verdicts).
#[derive(Debug, Clone, PartialEq)]
pub struct ProgressFact {
    /// The bound validator ref, when the item declares one.
    pub validator_ref: Option<String>,
    /// The latest verdict's affirmative read, when a verdict exists.
    pub latest_verdict_affirmative: Option<bool>,
    /// The latest verdict's record ref (the D8 evidence).
    pub verdict_ref: Option<String>,
    /// Whether the item is measured done.
    pub done: bool,
}

/// `RepetitionFact` — an idempotent-action repetition window (D10's
/// comparator — the caller folds `action.effect.*` into
/// `{signature, count, identical_observation, state_changed, refs, span}`
/// windows).
#[derive(Debug, Clone, PartialEq)]
pub struct RepetitionFact {
    /// The action signature the window groups on (capability semantic_id +
    /// canonical args).
    pub signature: String,
    /// The repetition count in the window.
    pub count: u64,
    /// Whether every observation in the window was identical.
    pub identical_observation: bool,
    /// Whether the authoritative state changed across the window.
    pub state_changed: bool,
    /// The window's record refs (the D10 evidence).
    pub refs: Vec<String>,
    /// The seqs the window spans `(first, last)` — a claim inside the span
    /// reads the window.
    pub span: (u64, u64),
}

/// `ReconcileContext` — the ledger-projected facts the C2 detectors read.
/// Every member is `environment`/`kernel`-authoritative by construction
/// (model text never enters — the caller's projection excludes
/// `delegate`-authority rows).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReconcileContext {
    /// D1 — subject-key → covering `context.observation.recorded`/
    /// `action.world_state.*` receipt ref. An `observed` claim whose
    /// subject has no receipt is a phantom observation.
    pub receipts: BTreeMap<String, String>,
    /// D4 — subject-key → last-changed watermark.
    pub watermarks: BTreeMap<String, Watermark>,
    /// D7 — refs present then forgotten (`context.compacted`/`forgotten`
    /// sets).
    pub forgotten_refs: BTreeSet<String>,
    /// D7 — refs present then truncated.
    pub truncated_refs: BTreeSet<String>,
    /// D7 — offloaded refs the model never read back.
    pub unread_offload_refs: BTreeSet<String>,
    /// D7 — refused effect refs (`effect:<id>` / `evt:*` shapes) — citing a
    /// refused outcome is a censored-evidence divergence.
    pub refused_effect_refs: BTreeSet<String>,
    /// D7 — subagent-run summary refs (`claimed` surfaces only — a summary
    /// never grounds a claim).
    pub delegate_summary_refs: BTreeSet<String>,
    /// D8 — progress-item key → measured fact.
    pub progress_items: BTreeMap<String, ProgressFact>,
    /// D10 — idempotent-action repetition windows.
    pub repetitions: Vec<RepetitionFact>,
}

impl ReconcileContext {
    /// The union of the D7 censor surfaces (forgotten ∪ truncated ∪
    /// unread-offload ∪ refused-effects ∪ delegate summaries).
    pub fn censored_refs(&self) -> BTreeSet<&String> {
        self.forgotten_refs
            .iter()
            .chain(self.truncated_refs.iter())
            .chain(self.unread_offload_refs.iter())
            .chain(self.refused_effect_refs.iter())
            .chain(self.delegate_summary_refs.iter())
            .collect()
    }
}

/// The canonical subject key — `kind:ref` (the key receipts/watermarks/
/// progress facts are indexed by).
pub fn subject_key(subject: &SubjectRef) -> String {
    match subject.ref_id() {
        Some(r) => format!("{}:{}", subject.kind_tag(), r),
        None => subject.kind_tag().to_string(),
    }
}

// ── reconcile_c2 — the C2 fold ───────────────────────────────────────────────

/// `reconcile_c2(claim, handles, ctx, decl, …) → [ReconciliationRecord]` —
/// the C2 reconcile fold (R-2.7.2b). The C0 `ledger_only` fold runs first;
/// when it diverges its record stands alone. Otherwise the declared C2
/// detectors run in D-order, each emitting its own record — the D7+D10 pair
/// AC-R-2.7.2b-2 names is two records, one per class.
///
/// - `enabled = false` ⇒ the C0 record alone (the ablation switch).
/// - `claim.kind ∉ decl.applies_to` (non-empty set) ⇒ C0 only.
/// - Every emitted record carries `detector = deterministic`,
///   `detector_ref = decl.reconciler_ref`, `charged_to = instrument`, and
///   `mode = ledger_only` (probe-mode records come from
///   [`reconcile_probe`]); a `diverge` always cites ≥ 1 record ref.
/// - `unverifiable` is the floor: a claim whose subject carries no C2 fact
///   and binds no handles stays `unverifiable`, never `agree`.
pub fn reconcile_c2(
    claim: &Claim,
    handles: &[AuthoritativeHandle],
    ctx: &ReconcileContext,
    decl: &ReconcilerDeclaration,
    reconciled_at_seq: u64,
    provenance: ProvenanceRecord,
) -> Vec<ReconciliationRecord> {
    let base = reconcile_ledger_only(
        claim,
        handles,
        &decl.reconciler_ref,
        reconciled_at_seq,
        provenance.clone(),
    );
    let applies =
        decl.enabled && (decl.applies_to.is_empty() || decl.applies_to.contains(&claim.kind));
    if !applies {
        return vec![base];
    }
    if matches!(base.agreement, Agreement::Diverge(c) if c.is_c0()) {
        // A C0 divergence decides the claim — the C2 classes only stack on
        // a non-divergent base.
        return vec![base];
    }

    let on_completion = matches!(claim.kind, ClaimKind::Achieved | ClaimKind::Unachievable)
        || matches!(claim.subject, SubjectRef::Run | SubjectRef::Criterion(_));
    let key = subject_key(&claim.subject);
    let mut records = Vec::new();
    let mut push_diverge = |class: DivergenceClass, evidence: Vec<String>| {
        let mut r = ReconciliationRecord {
            record_id: format!("recon:{}:{}", claim.claim_id, class.code()),
            claim_id: claim.claim_id.clone(),
            agreement: Agreement::Diverge(class),
            severity: severity_for(class, on_completion),
            detector: Detector::Deterministic,
            detector_ref: decl.reconciler_ref.clone(),
            confidence_ppm: 1_000_000,
            evidence_refs: evidence,
            probe_effect_ids: vec![],
            reconciled_at_seq,
            mode: ReconcileMode::LedgerOnly,
            charged_to: ChargedTo::Instrument,
            calibration_ref: None,
            independence_summary: None,
            provenance: provenance.clone(),
        };
        r.evidence_refs.sort();
        r.evidence_refs.dedup();
        records.push(r);
    };

    // D1 phantom_observation — an `observed` claim whose subject has no
    // covering receipt in the authoritative record.
    if decl
        .classes_detected
        .contains(&DivergenceClass::PhantomObservation)
        && claim.kind == ClaimKind::Observed
        && !ctx.receipts.contains_key(&key)
    {
        let mut ev: Vec<String> = claim.evidence_refs.clone();
        ev.extend(handles.iter().map(|h| h.handle_ref.clone()));
        if !ev.is_empty() {
            push_diverge(DivergenceClass::PhantomObservation, ev);
        }
        // No evidence to cite at all ⇒ the base `unverifiable` stands
        // (absence of proof is not proof of absence without a searched
        // record to name).
    }
    // D4 stale_belief — the claim's subject measurably changed after the
    // claim's `at_seq`.
    if decl
        .classes_detected
        .contains(&DivergenceClass::StaleBelief)
    {
        if let Some(wm) = ctx.watermarks.get(&key) {
            if wm.changed_at_seq > claim.at_seq {
                push_diverge(DivergenceClass::StaleBelief, vec![wm.change_ref.clone()]);
            }
        }
    }
    // D7 censored_evidence — the claim cites a record the censor surfaces
    // (forgotten/truncated/unread-offload/refused/summary-only) cover.
    if decl
        .classes_detected
        .contains(&DivergenceClass::CensoredEvidence)
    {
        let censored = ctx.censored_refs();
        let hits: Vec<String> = claim
            .evidence_refs
            .iter()
            .filter(|r| censored.contains(r))
            .cloned()
            .collect();
        if !hits.is_empty() {
            push_diverge(DivergenceClass::CensoredEvidence, hits);
        }
    }
    // D8 progress_regression — a `progress`/`achieved` claim on an item
    // whose measured fact shows a regression (a negative latest verdict,
    // an un-done item, or a never-run check).
    if decl
        .classes_detected
        .contains(&DivergenceClass::ProgressRegression)
        && matches!(claim.kind, ClaimKind::Progress | ClaimKind::Achieved)
    {
        if let Some(fact) = ctx.progress_items.get(&key) {
            let regressed = !fact.done
                && match fact.latest_verdict_affirmative {
                    Some(affirmative) => !affirmative,
                    None => true, // claimed progress on a never-checked item
                };
            if regressed {
                let mut ev = Vec::new();
                if let Some(v) = &fact.verdict_ref {
                    ev.push(v.clone());
                }
                if let Some(v) = &fact.validator_ref {
                    ev.push(v.clone());
                }
                if ev.is_empty() {
                    ev.push(format!("progress_item:{}", key));
                }
                push_diverge(DivergenceClass::ProgressRegression, ev);
            }
        }
    }
    // D9 evidence_inversion — deterministic arm: a cited handle whose
    // closed-schema value contradicts the claim outside the C0 classes
    // (the judged arm covers free-text inversions — see
    // [`reconcile_judged`]).
    if decl
        .classes_detected
        .contains(&DivergenceClass::EvidenceInversion)
    {
        let hits: Vec<String> = handles
            .iter()
            .filter(|h| {
                h.produced_at_seq <= claim.at_seq
                    && h.value
                        .as_ref()
                        .is_some_and(|v| claim_contradicted_open(claim, v))
            })
            .map(|h| h.handle_ref.clone())
            .collect();
        if !hits.is_empty() {
            push_diverge(DivergenceClass::EvidenceInversion, hits);
        }
    }
    // D10 no_progress_loop — ≥ k repeated idempotent actions, identical
    // observations, no state change, claim inside the window.
    if decl
        .classes_detected
        .contains(&DivergenceClass::NoProgressLoop)
        && matches!(
            claim.kind,
            ClaimKind::Effected | ClaimKind::Progress | ClaimKind::Pending | ClaimKind::Observed
        )
    {
        let k = decl.no_progress_k.max(1);
        if let Some(win) = ctx.repetitions.iter().find(|w| {
            w.count >= k
                && w.identical_observation
                && !w.state_changed
                && claim.at_seq >= w.span.0
                && claim.at_seq <= w.span.1
        }) {
            push_diverge(DivergenceClass::NoProgressLoop, win.refs.clone());
        }
    }

    if records.is_empty() {
        vec![base]
    } else {
        records
    }
}

/// The open contradiction predicate the deterministic D9 arm uses — a cited
/// handle's closed-schema value directly contradicting the claim outside
/// the C0 classes (`Value` mismatches on non-`effected` kinds).
fn claim_contradicted_open(claim: &Claim, value: &crate::claims::HandleValue) -> bool {
    use crate::claims::HandleValue::*;
    match (claim.kind, value) {
        (ClaimKind::Effected, Value(v)) | (_, Value(v)) => *v != claim.asserted,
        (ClaimKind::Verified, Verdict { affirmative }) => !affirmative,
        (ClaimKind::Pending, EffectState { terminal, .. }) => *terminal,
        (
            ClaimKind::Observed,
            EffectState {
                refused, outcome, ..
            },
        ) => *refused || outcome == "error",
        _ => false,
    }
}

// ── reconcile_judged — the judged arm ────────────────────────────────────────

/// `JudgedAssessment` — what a judged detector asserts about a claim (the
/// input `reconcile_judged` folds; the judge itself is the caller's seam).
#[derive(Debug, Clone, PartialEq)]
pub struct JudgedAssessment {
    /// The divergence classes the judge detected (intersected with the
    /// declaration's `classes_detected` — an undeclared class never emits).
    pub classes: Vec<DivergenceClass>,
    /// The record refs the assessment cites.
    pub evidence_refs: Vec<String>,
    /// The judge's confidence in ppm.
    pub confidence_ppm: u64,
    /// Whether the judge affirms the claim (emitted only when `classes` is
    /// empty — a judged agree is a delegate-authoritative `agree`, never a
    /// gate fact).
    pub agree: bool,
}

/// `reconcile_judged(claim, assessment, decl, …) → [ReconciliationRecord]` —
/// the `detector = judged` arm (AC-R-2.7.2b-1/3): each detected class emits
/// a delegate-authoritative record carrying `calibration_ref` +
/// `independence_summary` (the declaration's — mandatory by
/// [`declare_reconciler`]). Judged records are veto-admissible, never
/// hold-admissible (F7 — [`ReconciliationRecord::hold_admissible`]).
pub fn reconcile_judged(
    claim: &Claim,
    assessment: &JudgedAssessment,
    decl: &ReconcilerDeclaration,
    reconciled_at_seq: u64,
    provenance: ProvenanceRecord,
) -> Result<Vec<ReconciliationRecord>, ReconcilerError> {
    if !decl.detector_classes.contains(&Detector::Judged) {
        return Ok(vec![]);
    }
    let calibration = decl
        .calibration_ref
        .clone()
        .ok_or(ReconcilerError::JudgedWithoutCalibration)?;
    let judge_ref = decl
        .judge_ref
        .clone()
        .unwrap_or_else(|| decl.reconciler_ref.clone());
    let mut records = Vec::new();
    let on_completion = matches!(claim.kind, ClaimKind::Achieved | ClaimKind::Unachievable)
        || matches!(claim.subject, SubjectRef::Run | SubjectRef::Criterion(_));
    let mk =
        |id_suffix: String, agreement: Agreement, evidence: Vec<String>| -> ReconciliationRecord {
            ReconciliationRecord {
                record_id: format!("recon:{}:{id_suffix}", claim.claim_id),
                claim_id: claim.claim_id.clone(),
                agreement,
                severity: match agreement {
                    Agreement::Diverge(c) => severity_for(c, on_completion),
                    _ => SeverityLevel::Info,
                },
                detector: Detector::Judged,
                detector_ref: judge_ref.clone(),
                confidence_ppm: assessment.confidence_ppm,
                evidence_refs: evidence,
                probe_effect_ids: vec![],
                reconciled_at_seq,
                mode: ReconcileMode::LedgerOnly,
                charged_to: ChargedTo::Instrument,
                calibration_ref: Some(calibration.clone()),
                independence_summary: decl.independence_summary.clone(),
                provenance: provenance.clone(),
            }
        };
    for class in &assessment.classes {
        if !decl.classes_detected.contains(class) {
            continue;
        }
        let mut r = mk(
            format!("judged:{}", class.code()),
            Agreement::Diverge(*class),
            assessment.evidence_refs.clone(),
        );
        if r.evidence_refs.is_empty() {
            r.evidence_refs.push(claim.claim_id.clone());
        }
        records.push(r);
    }
    if assessment.classes.is_empty() && assessment.agree {
        records.push(mk("judged".to_string(), Agreement::Agree, vec![]));
    }
    Ok(records)
}

// ── probe mode (R-2.7.2b; OQ-289) ────────────────────────────────────────────

/// `ProbeOutcome` — one `action.probe` result: the ordinary-effect id it
/// ran as plus the authoritative handle the probe produced (the probe's
/// result enters the reconciliation evidence bundle).
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeOutcome {
    /// The effect id the probe ran as.
    pub effect_id: String,
    /// The handle the probe produced.
    pub handle: AuthoritativeHandle,
}

/// `ProbeError` — the probe dispatch refusals (`probe_denied` exhausts to
/// `unverifiable`, never `agree` — AC-R-2.7.2b-5).
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeError {
    /// Π denied the probe (the capability is undeclared, not read-only, or
    /// not closed-world at this site).
    Denied {
        /// The denied capability.
        capability_ref: String,
        /// The denial detail.
        detail: String,
    },
    /// The probe budget share (`reconciler.probe_budget_share_cap`) was
    /// exhausted — further probes are denied until the budget window rolls.
    BudgetExhausted,
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeError::Denied {
                capability_ref,
                detail,
            } => write!(f, "ProbeDenied: {capability_ref} — {detail}"),
            ProbeError::BudgetExhausted => write!(f, "ProbeDenied: probe budget exhausted"),
        }
    }
}

impl std::error::Error for ProbeError {}

/// `ProbePort` — the caller's probe dispatch seam. A probe is an
/// `action.probe` planned step dispatched through the *ordinary* effect
/// lifecycle (the port's implementation emits `action.effect.intended` →
/// `action.probe.executed` with `charged_to` per OQ-289).
pub trait ProbePort {
    /// Dispatch one probe; the outcome is the probe's authoritative handle.
    fn probe(&mut self, capability_ref: &str, args: &Json) -> Result<ProbeOutcome, ProbeError>;
}

/// `reconcile_probe(claim, probes, ctx, decl, port, charged_to, …) →
/// [ReconciliationRecord]` — `probe` mode (AC-R-2.7.2b-5): each planned
/// probe dispatches through `port` as an ordinary effect; the returned
/// handles join the bound handles and the C2 fold runs over the union.
/// Emitted records carry `mode = probe` + `probe_effect_ids[]` + the
/// declared `charged_to` (`subject` when bound, `instrument` Lab-invoked —
/// the caller decides). A denied probe leaves the claim `unverifiable`
/// (never `agree`); a probe on an unbound claim is `unverifiable`.
#[allow(clippy::too_many_arguments)] // the probe-mode record's inputs are its shape.
pub fn reconcile_probe(
    claim: &Claim,
    handles: &[AuthoritativeHandle],
    ctx: &ReconcileContext,
    decl: &ReconcilerDeclaration,
    probes: &[(String, Json)],
    port: &mut dyn ProbePort,
    charged_to: ChargedTo,
    reconciled_at_seq: u64,
    provenance: ProvenanceRecord,
) -> Vec<ReconciliationRecord> {
    let mut all_handles: Vec<AuthoritativeHandle> = handles.to_vec();
    let mut effect_ids = Vec::new();
    let mut denied = false;
    for (cap, args) in probes {
        match port.probe(cap, args) {
            Ok(outcome) => {
                effect_ids.push(outcome.effect_id.clone());
                all_handles.push(outcome.handle);
            }
            Err(_) => denied = true,
        }
    }
    let mut records = reconcile_c2(
        claim,
        &all_handles,
        ctx,
        decl,
        reconciled_at_seq,
        provenance,
    );
    for r in &mut records {
        r.mode = ReconcileMode::Probe;
        r.probe_effect_ids = effect_ids.clone();
        r.charged_to = charged_to;
        // A denied probe never lets the fold claim `agree` — downgrade to
        // `unverifiable` (the refusal is itself a measured fact).
        if denied && matches!(r.agreement, Agreement::Agree) {
            r.agreement = Agreement::Unverifiable;
            r.severity = SeverityLevel::Low;
        }
    }
    records
}

// ── kernel_notice feed-back (F6) ─────────────────────────────────────────────

/// `feed_back_notices(records, claims, delivered)` — the mid-run
/// `kernel_notice` projection (AC-R-2.7.2b-2): each diverging record
/// yields at most one notice per `(class, subject)` per turn — `delivered`
/// is the caller-maintained per-turn set. `suggested` names the concrete
/// repair (a D7 citing a refused effect names the refusal; D1/D4 name
/// `re_observe`; D10 names `change_strategy`).
pub fn feed_back_notices<'a>(
    records: &[ReconciliationRecord],
    claims: impl Iterator<Item = &'a Claim>,
    delivered: &mut BTreeSet<(DivergenceClass, String)>,
) -> Vec<ReconciliationNotice> {
    let claim_of: BTreeMap<&str, &Claim> = claims.map(|c| (c.claim_id.as_str(), c)).collect();
    let mut out = Vec::new();
    for r in records {
        let Agreement::Diverge(class) = r.agreement else {
            continue;
        };
        let Some(claim) = claim_of.get(r.claim_id.as_str()) else {
            continue;
        };
        let key = (class, subject_key(&claim.subject));
        if !delivered.insert(key) {
            continue; // already delivered this turn (F6 dedup)
        }
        let mut suggested = Vec::new();
        match class {
            DivergenceClass::PhantomObservation | DivergenceClass::StaleBelief => {
                suggested.push(format!("re_observe({})", subject_key(&claim.subject)));
            }
            DivergenceClass::CensoredEvidence => {
                for ev in &r.evidence_refs {
                    suggested.push(format!("re_observe({ev})"));
                    suggested.push(format!("respect_refusal({ev})"));
                }
            }
            DivergenceClass::ProgressRegression => {
                suggested.push(format!("re_check({})", subject_key(&claim.subject)));
            }
            DivergenceClass::NoProgressLoop => {
                suggested.push("change_strategy".to_string());
            }
            _ => {
                suggested.push("review_evidence".to_string());
            }
        }
        out.push(ReconciliationNotice {
            record_ref: r.record_id.clone(),
            class,
            subject: claim.subject.clone(),
            expected: r
                .evidence_refs
                .first()
                .cloned()
                .unwrap_or_else(|| "unverifiable".to_string()),
            observed: claim.asserted.to_canonical_string(),
            suggested,
        });
    }
    out
}

// ── outside-gate intervention cap ────────────────────────────────────────────

/// `cap_outside_gate(intervention, severity, irreversible_external)` — the
/// mid-run intervention ceiling (AC-R-2.7.2b-6): outside the gate Γ
/// resolves `annotate|feed_back`; `escalate|stop` survive only at
/// `severity = critical` on an irreversible-external subject (the
/// §05a-owned kill path). Every stronger intervention clamps to
/// `feed_back`.
pub fn cap_outside_gate(
    intervention: Intervention,
    severity: SeverityLevel,
    irreversible_external: bool,
) -> Intervention {
    match intervention {
        Intervention::Escalate | Intervention::Stop
            if severity == SeverityLevel::Critical && irreversible_external =>
        {
            intervention
        }
        i if i > Intervention::FeedBack => Intervention::FeedBack,
        i => i,
    }
}

// ── Γ as HarnessRule definition data ─────────────────────────────────────────

/// `gamma_from_rules(rules) → Gamma` — the `HarnessRuleRecord` → Γ-row
/// projection (AC-R-2.7.2b-5): a Γ row is a `HarnessRule` whose `trigger`
/// carries `{kind: "intervention", divergence_class?, severity_floor?,
/// decision_point?}` and whose `scope` carries `{intervention: <spelling>}`.
/// `conditioned_on` maps to `profile_conditioned` (such a row must carry
/// `assumption_debt` — [`crate::gate::validate_gamma`] refuses it
/// otherwise). Non-γ rules pass through untouched.
pub fn gamma_from_rules(rules: &[hh_hir::records::HarnessRuleRecord]) -> Result<Gamma, GammaError> {
    let mut gamma = Gamma::default();
    for rule in rules {
        let is_gamma = matches!(
            rule.trigger.get("kind"),
            Some(Json::Str(k)) if k == "intervention"
        );
        if !is_gamma {
            continue;
        }
        let divergence_class = match rule.trigger.get("divergence_class") {
            Some(Json::Str(s)) => DivergenceClass::parse(s),
            _ => None,
        };
        let severity_floor = match rule.trigger.get("severity_floor") {
            Some(Json::Str(s)) => SeverityLevel::parse(s),
            _ => None,
        };
        let decision_point = match rule.trigger.get("decision_point") {
            Some(Json::Str(s)) => match s.as_str() {
                "stop" => Some(DecisionPoint::Stop),
                "verify" => Some(DecisionPoint::Verify),
                "authorize" => Some(DecisionPoint::Authorize),
                "compact" => Some(DecisionPoint::Compact),
                "delegate" => Some(DecisionPoint::Delegate),
                _ => None,
            },
            _ => None,
        };
        let intervention = match rule.scope.get("intervention") {
            Some(Json::Str(s)) => Intervention::parse(s).unwrap_or(Intervention::Annotate),
            _ => Intervention::Annotate,
        };
        gamma.rows.push(GammaRow {
            divergence_class,
            severity_floor,
            decision_point,
            intervention,
            rule_ref: rule.rule_id.clone(),
            assumption_debt: rule.assumption_debt.as_ref().map(|d| d.rule_id.clone()),
            profile_conditioned: rule.conditioned_on.is_some(),
        });
    }
    crate::gate::validate_gamma(&gamma)?;
    Ok(gamma)
}

/// `RemovalRecipe` — the assumption-debt removal test a conditioned γ row's
/// debt names; the "disable Γ for model M" recipe executes as the Lab's
/// `interventions_experiment` (AC-R-2.7.2b-5).
#[derive(Debug, Clone, PartialEq)]
pub struct RemovalRecipe {
    /// The recipe kind (`"removal_test"`).
    pub kind: String,
    /// What is removed (`"gamma_rows"`).
    pub target: String,
    /// The model selector the removal is scoped to.
    pub model_selector: String,
    /// The experiment that executes it.
    pub experiment: String,
}

/// `disable_gamma_recipe(model_selector) → RemovalRecipe` — the
/// "disable Γ for model M" recipe (AC-R-2.7.2b-5): a
/// `removal_test{target: gamma_rows}` scoped to the selector, executed as
/// `interventions_experiment{model_selector}` by the Lab.
pub fn disable_gamma_recipe(model_selector: &str) -> RemovalRecipe {
    RemovalRecipe {
        kind: "removal_test".to_string(),
        target: "gamma_rows".to_string(),
        model_selector: model_selector.to_string(),
        experiment: "interventions_experiment".to_string(),
    }
}

// ── hosted gate (floor 9 — C1's D5-shape over end_state) ─────────────────────

/// `EndStateAssertion` — one declared end-state assertion vs the
/// environment's measured `end_state` read (the hosted participant's
/// criterion handle — `HandleKind::EnvironmentProbe`/`end_state` reads).
#[derive(Debug, Clone, PartialEq)]
pub struct EndStateAssertion {
    /// The assertion ref (`semantic_id`-aligned).
    pub assertion_ref: String,
    /// The declared end-state value.
    pub declared: Json,
    /// The measured end-state value, when the environment read ran.
    pub measured: Option<Json>,
    /// The measured read's record ref (the evidence).
    pub measured_ref: Option<String>,
}

/// `HostedGateFacts` — floor-9's inputs: the completion claim's
/// kind/agreement plus the declared-vs-measured `end_state` table (the
/// hosted participant's D5 shape — every fact is an `end_state` handle the
/// caller projected).
#[derive(Debug, Clone, PartialEq)]
pub struct HostedGateFacts {
    /// The completion claim's kind.
    pub completion_claim_kind: ClaimKind,
    /// The completion claim's agreement (a reconciled `unachievable` is the
    /// honest-failure path).
    pub completion_claim_agreement: Agreement,
    /// The declared end-state assertions vs measured reads.
    pub end_state_assertions: Vec<EndStateAssertion>,
    /// Holds already consumed.
    pub holds_consumed: u64,
    /// The `reconciliation.holds` cap.
    pub holds_cap: u64,
}

/// `evaluate_gate_hosted(facts) → GateResult` — the hosted floor-9 verdict
/// (AC-R-2.7.1²/R-2.7.2b-7): `pass` when every declared end-state assertion
/// measured `=`; `hold{contract_gap}` on an unmet/unmeasured assertion
/// (each hold consumes `reconciliation.holds`); `unachievable +
/// agree/unverifiable` is the honest-failure pass.
pub fn evaluate_gate_hosted(facts: &HostedGateFacts) -> GateResult {
    let mut divergences = Vec::new();
    let mut required = Vec::new();
    let mut evidence = Vec::new();
    for a in &facts.end_state_assertions {
        match (&a.measured, &a.measured_ref) {
            (Some(m), Some(r)) if *m == a.declared => {
                evidence.push(r.clone());
            }
            (Some(_), Some(r)) => {
                divergences.push(DivergenceClass::ContractGap);
                required.push(format!("end_state_unmet:{}", a.assertion_ref));
                evidence.push(r.clone());
            }
            _ => {
                divergences.push(DivergenceClass::ContractGap);
                required.push(format!("end_state_unread:{}", a.assertion_ref));
            }
        }
    }
    let honest_failure = facts.completion_claim_kind == ClaimKind::Unachievable
        && matches!(
            facts.completion_claim_agreement,
            Agreement::Agree | Agreement::Unverifiable
        );
    let hold_count = if divergences.is_empty() {
        facts.holds_consumed
    } else {
        facts.holds_consumed + 1
    };
    let holds_exhausted = hold_count > facts.holds_cap;
    let verdict = if divergences.is_empty() {
        GateVerdict::Pass
    } else {
        GateVerdict::Hold {
            divergences,
            required_actions: required,
        }
    };
    GateResult {
        verdict,
        hold_count,
        evidence_refs: evidence,
        holds_exhausted,
        honest_failure,
        success_with_veto: None,
    }
}

// ── AC-R-2.7.3-12 — the per-class n/a table for hosted participants ──────────

/// `na_classes(decl, available) → [divergence class]` — the C2 classes the
/// participant's observability cannot support (AC-R-2.7.3-12's per-class
/// `n/a{observability}` table):
///
/// - `d1` deterministic needs `events` or `end_state` (a receipt surface).
/// - `d4` needs `events`/`ledger`/`end_state` (a change fact).
/// - `d7` needs `events`/`ledger` (forgotten/truncated facts are ledger
///   events).
/// - `d8` needs `events`/`ledger` (progress artifacts + verdicts).
/// - `d9` needs `events`/`end_state`/`ledger` (cited evidence bytes).
/// - `d10` needs `events`/`ledger` (the action stream).
/// - `ledger` implies `events`; `judged` arms additionally need
///   `model_io` or `end_state` (a surface the judge can see).
pub fn na_classes(available: &BTreeSet<Observability>, detector: Detector) -> Vec<DivergenceClass> {
    let has_events =
        available.contains(&Observability::Events) || available.contains(&Observability::Ledger);
    let has_state = has_events || available.contains(&Observability::EndState);
    // AC-R-2.7.3-12: a judged detector reads `model_io` — a session-ABI
    // row exposing `end_state` only renders every judged class
    // `n/a{observability}`; interception rows (`model_io`) populate.
    let judge_surface = available.contains(&Observability::ModelIo);
    let mut out = Vec::new();
    let mut need = |class: DivergenceClass, ok: bool| {
        if !ok {
            out.push(class);
        }
    };
    need(DivergenceClass::PhantomObservation, has_state);
    need(DivergenceClass::StaleBelief, has_state);
    need(DivergenceClass::CensoredEvidence, has_events);
    need(DivergenceClass::ProgressRegression, has_events);
    need(DivergenceClass::EvidenceInversion, has_state);
    need(DivergenceClass::NoProgressLoop, has_events);
    if detector == Detector::Judged {
        // A judged detector additionally needs a surface the judge reads.
        if !judge_surface {
            out.extend([
                DivergenceClass::PhantomObservation,
                DivergenceClass::StaleBelief,
                DivergenceClass::CensoredEvidence,
                DivergenceClass::ProgressRegression,
                DivergenceClass::EvidenceInversion,
                DivergenceClass::NoProgressLoop,
            ]);
            out.sort();
            out.dedup();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claims::{Claim, CriterionState, HandleValue};
    use crate::vocab::{ExtractedBy, HandleKind, ReconcileMode};
    use hh_provenance::authority::AuthorityClass;
    use hh_provenance::authority::PersistenceScope;
    use hh_provenance::origin::Origin;

    fn delegate_prov() -> ProvenanceRecord {
        ProvenanceRecord::minted(
            Origin::model("model/x", "snapshot:1", "call:1"),
            PersistenceScope::Run,
            7,
        )
    }

    fn kernel_prov() -> ProvenanceRecord {
        ProvenanceRecord::kernel("hir/kernel/reconcile", 3)
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
            handle_ref: format!("evt:{:?}:{seq}", kind),
            value: Some(value),
            authority: AuthorityClass::Kernel,
            produced_at_seq: seq,
        }
    }

    /// The full-C2 declaration (all six classes, deterministic only).
    fn decl() -> ReconcilerDeclaration {
        ReconcilerDeclaration {
            reconciler_ref: "reconciler/exec-align.v1".into(),
            classes_detected: [
                DivergenceClass::PhantomObservation,
                DivergenceClass::StaleBelief,
                DivergenceClass::CensoredEvidence,
                DivergenceClass::ProgressRegression,
                DivergenceClass::EvidenceInversion,
                DivergenceClass::NoProgressLoop,
            ]
            .into_iter()
            .collect(),
            detector_classes: [Detector::Deterministic].into_iter().collect(),
            probe_capabilities: vec![ProbeCapability::read_only("cap/fs_read.probe")],
            requires_observability: [Observability::Events].into_iter().collect(),
            applies_to: BTreeSet::new(),
            probe_budget_share_cap_ppm: None,
            conditioned_on: None,
            assumption_debt: None,
            judge_ref: None,
            calibration_ref: None,
            independence_summary: None,
            no_progress_k: 2,
            enabled: true,
        }
    }

    // AC-R-2.7.2b-1 — D1: an `observed` claim with no covering receipt.
    #[test]
    fn d1_phantom_observation() {
        let c = claim(
            ClaimKind::Observed,
            SubjectRef::File("src/main.rs".into()),
            Json::str("exists"),
        );
        c.validate().unwrap();
        let mut c = c;
        c.evidence_refs = vec!["evt:9".into()];
        let recs = reconcile_c2(
            &c,
            &[],
            &ReconcileContext::default(),
            &decl(),
            20,
            kernel_prov(),
        );
        assert_eq!(recs.len(), 1);
        assert_eq!(
            recs[0].agreement,
            Agreement::Diverge(DivergenceClass::PhantomObservation)
        );
        assert_eq!(recs[0].detector, Detector::Deterministic);
        assert!(!recs[0].evidence_refs.is_empty());
        // With a covering receipt the claim reconciles clean.
        let mut ctx = ReconcileContext::default();
        ctx.receipts
            .insert(subject_key(&c.subject), "evt:11".into());
        let recs = reconcile_c2(&c, &[], &ctx, &decl(), 20, kernel_prov());
        assert_eq!(recs.len(), 1);
        assert!(!matches!(
            recs[0].agreement,
            Agreement::Diverge(DivergenceClass::PhantomObservation)
        ));
    }

    // AC-R-2.7.2b-1 — D4: a watermark newer than the claim's `at_seq`.
    #[test]
    fn d4_stale_belief() {
        let c = claim(
            ClaimKind::Observed,
            SubjectRef::File("src/lib.rs".into()),
            Json::str("unchanged"),
        );
        let mut ctx = ReconcileContext::default();
        ctx.receipts.insert(subject_key(&c.subject), "evt:8".into());
        ctx.watermarks.insert(
            subject_key(&c.subject),
            Watermark {
                changed_at_seq: 15,
                change_ref: "evt:15".into(),
            },
        );
        let recs = reconcile_c2(&c, &[], &ctx, &decl(), 20, kernel_prov());
        assert!(recs.iter().any(|r| r.agreement
            == Agreement::Diverge(DivergenceClass::StaleBelief)
            && r.evidence_refs.contains(&"evt:15".to_string())));
    }

    // AC-R-2.7.2b-2 — D7 + D10: a compaction forgetting cited evidence,
    // plus the refused-action retry window, yields the pair and the
    // notices name the refusal.
    #[test]
    fn d7_d10_pair_and_notices() {
        let mut c = claim(
            ClaimKind::Observed,
            SubjectRef::File("src/a.rs".into()),
            Json::str("read"),
        );
        c.evidence_refs = vec!["evt:5".into()];
        let mut ctx = ReconcileContext::default();
        ctx.receipts
            .insert(subject_key(&c.subject), "evt:11".into());
        ctx.forgotten_refs.insert("evt:5".into());
        ctx.repetitions.push(RepetitionFact {
            signature: "fs_read:src/a.rs".into(),
            count: 3,
            identical_observation: true,
            state_changed: false,
            refs: vec!["evt:6".into(), "evt:7".into()],
            span: (5, 12),
        });
        let recs = reconcile_c2(&c, &[], &ctx, &decl(), 20, kernel_prov());
        let classes: Vec<DivergenceClass> = recs
            .iter()
            .filter_map(|r| match r.agreement {
                Agreement::Diverge(c) => Some(c),
                _ => None,
            })
            .collect();
        assert!(classes.contains(&DivergenceClass::CensoredEvidence));
        assert!(classes.contains(&DivergenceClass::NoProgressLoop));

        // Notices: one per (class, subject); the D7 notice names the
        // forgotten ref.
        let mut delivered = BTreeSet::new();
        let notices = feed_back_notices(&recs, [&c].into_iter(), &mut delivered);
        assert_eq!(notices.len(), 2);
        let d7 = notices
            .iter()
            .find(|n| n.class == DivergenceClass::CensoredEvidence)
            .unwrap();
        assert!(d7.suggested.iter().any(|s| s.contains("evt:5")));
        // Dedup: a second call yields nothing.
        assert!(feed_back_notices(&recs, [&c].into_iter(), &mut delivered).is_empty());
    }

    // AC-R-2.7.2b-1 — D8: claimed progress on a measured-regressed item.
    #[test]
    fn d8_progress_regression() {
        let c = claim(
            ClaimKind::Progress,
            SubjectRef::ProgressItem("item-1".into()),
            Json::str("done"),
        );
        let mut ctx = ReconcileContext::default();
        ctx.progress_items.insert(
            subject_key(&c.subject),
            ProgressFact {
                validator_ref: Some("validator/build".into()),
                latest_verdict_affirmative: Some(false),
                verdict_ref: Some("verdict:build:9".into()),
                done: false,
            },
        );
        let recs = reconcile_c2(&c, &[], &ctx, &decl(), 20, kernel_prov());
        assert!(recs.iter().any(|r| r.agreement
            == Agreement::Diverge(DivergenceClass::ProgressRegression)
            && r.evidence_refs.contains(&"verdict:build:9".to_string())));
    }

    // AC-R-2.7.2b-1 — D9 deterministic arm: a cited kernel handle whose
    // closed-schema value contradicts the claim.
    #[test]
    fn d9_evidence_inversion() {
        // An `observed` claim + a contradicting closed-schema value is the
        // open C2 contradiction (`Effected` resolves to the C0 D2 class).
        let c = claim(
            ClaimKind::Observed,
            SubjectRef::EnvironmentState("queue".into()),
            Json::str("empty"),
        );
        let h = handle(
            HandleKind::EnvironmentProbe,
            HandleValue::Value(Json::str("nonempty")),
            8,
        );
        let recs = reconcile_c2(
            &c,
            &[h],
            &ReconcileContext::default(),
            &decl(),
            20,
            kernel_prov(),
        );
        assert!(recs
            .iter()
            .any(|r| r.agreement == Agreement::Diverge(DivergenceClass::EvidenceInversion)));
    }

    // AC-R-2.7.2b-1 — D10: a claim inside a no-progress repetition window.
    #[test]
    fn d10_no_progress_loop() {
        let c = claim(ClaimKind::Pending, SubjectRef::Run, Json::str("retry"));
        let mut ctx = ReconcileContext::default();
        ctx.repetitions.push(RepetitionFact {
            signature: "exec:ls".into(),
            count: 2,
            identical_observation: true,
            state_changed: false,
            refs: vec!["evt:3".into()],
            span: (1, 11),
        });
        let recs = reconcile_c2(&c, &[], &ctx, &decl(), 20, kernel_prov());
        assert!(recs
            .iter()
            .any(|r| r.agreement == Agreement::Diverge(DivergenceClass::NoProgressLoop)));
    }

    // The ablation switch: `enabled = false` yields the C0 record alone.
    #[test]
    fn disabled_reconciler_is_byte_identical_c0() {
        let c = claim(
            ClaimKind::Observed,
            SubjectRef::File("src/x".into()),
            Json::str("x"),
        );
        let mut d = decl();
        d.enabled = false;
        let recs = reconcile_c2(&c, &[], &ReconcileContext::default(), &d, 20, kernel_prov());
        assert_eq!(recs.len(), 1);
        let c0 = reconcile_ledger_only(&c, &[], &d.reconciler_ref, 20, kernel_prov());
        assert_eq!(recs[0].agreement, c0.agreement);
        assert_eq!(recs[0].detector_ref, c0.detector_ref);
    }

    // A C0 divergence stands alone — C2 classes never stack on it.
    #[test]
    fn c0_divergence_decides() {
        // PhantomEffect: an `effected` claim contradicted by a refused
        // effect-state handle — the C0 fold diverges; the C2 detectors
        // stay silent (the D7 surface is populated and still no pair).
        let c = claim(
            ClaimKind::Effected,
            SubjectRef::Effect("e9".into()),
            Json::str("applied"),
        );
        let h = handle(
            HandleKind::EffectState,
            HandleValue::EffectState {
                effect_id: "e9".into(),
                terminal: true,
                refused: true,
                outcome: "refused".into(),
            },
            8,
        );
        let mut ctx = ReconcileContext::default();
        ctx.forgotten_refs.insert("evt:9".into());
        let recs = reconcile_c2(&c, &[h], &ctx, &decl(), 20, kernel_prov());
        assert_eq!(recs.len(), 1);
        assert!(matches!(
            recs[0].agreement,
            Agreement::Diverge(c) if c.is_c0()
        ));
    }

    // declare_reconciler — the seal-time refusals (AC-R-2.7.2b-1).
    #[test]
    fn declare_refusals() {
        let mut d = decl();
        d.probe_capabilities = vec![ProbeCapability {
            capability_ref: "cap/w".into(),
            read_only: false,
            closed_world: true,
        }];
        assert!(matches!(
            declare_reconciler(&d),
            Err(ReconcilerError::ProbeNotReadOnly { .. })
        ));
        let mut d = decl();
        d.probe_capabilities = vec![ProbeCapability {
            capability_ref: "cap/w".into(),
            read_only: true,
            closed_world: false,
        }];
        assert!(matches!(
            declare_reconciler(&d),
            Err(ReconcilerError::ProbeNotClosedWorld { .. })
        ));
        let mut d = decl();
        d.conditioned_on = Some("profile/x".into());
        assert_eq!(
            declare_reconciler(&d),
            Err(ReconcilerError::ConditionedWithoutDebt)
        );
        let mut d = decl();
        d.detector_classes.insert(Detector::Judged);
        assert_eq!(
            declare_reconciler(&d),
            Err(ReconcilerError::JudgedWithoutCalibration)
        );
        declare_reconciler(&decl()).unwrap();
    }

    // AC-R-2.7.2b-3 — judged records: delegate authority, calibration +
    // independence carried, never hold-admissible.
    #[test]
    fn judged_records_never_hold() {
        let mut d = decl();
        d.detector_classes.insert(Detector::Judged);
        d.judge_ref = Some("judge/belief.v1".into());
        d.calibration_ref = Some("calib/belief.v1".into());
        d.independence_summary = Some("snapshot=different_family".into());
        let c = claim(ClaimKind::Progress, SubjectRef::Run, Json::str("p"));
        let assessment = JudgedAssessment {
            classes: vec![DivergenceClass::NoProgressLoop],
            evidence_refs: vec!["evt:4".into()],
            confidence_ppm: 800_000,
            agree: false,
        };
        let recs = reconcile_judged(&c, &assessment, &d, 20, kernel_prov()).unwrap();
        assert_eq!(recs.len(), 1);
        let r = &recs[0];
        assert_eq!(r.detector, Detector::Judged);
        assert_eq!(r.calibration_ref.as_deref(), Some("calib/belief.v1"));
        assert!(r.independence_summary.is_some());
        assert!(!r.hold_admissible());
        r.validate().unwrap();
        // An undeclared class never emits.
        let assessment = JudgedAssessment {
            classes: vec![DivergenceClass::ContractGap],
            evidence_refs: vec![],
            confidence_ppm: 900_000,
            agree: false,
        };
        assert!(reconcile_judged(&c, &assessment, &d, 20, kernel_prov())
            .unwrap()
            .is_empty());
    }

    // AC-R-2.7.2b-5 — probe mode: handles join the fold; a denied probe
    // leaves the claim unverifiable, never agree.
    #[test]
    fn probe_mode() {
        struct Port {
            deny: bool,
        }
        impl ProbePort for Port {
            fn probe(&mut self, cap: &str, _args: &Json) -> Result<ProbeOutcome, ProbeError> {
                if self.deny {
                    return Err(ProbeError::Denied {
                        capability_ref: cap.to_string(),
                        detail: "not declared".into(),
                    });
                }
                Ok(ProbeOutcome {
                    effect_id: "probe:e1".into(),
                    handle: AuthoritativeHandle {
                        kind: HandleKind::EnvironmentProbe,
                        handle_ref: "probe:e1".into(),
                        value: Some(HandleValue::Value(Json::str("exists"))),
                        authority: AuthorityClass::Kernel,
                        produced_at_seq: 19,
                    },
                })
            }
        }
        let c = claim(
            ClaimKind::Observed,
            SubjectRef::File("src/p".into()),
            Json::str("exists"),
        );
        // A denied probe → unverifiable, never agree.
        let mut port = Port { deny: true };
        let recs = reconcile_probe(
            &c,
            &[],
            &ReconcileContext::default(),
            &decl(),
            &[("cap/fs_read.probe".into(), Json::Null)],
            &mut port,
            ChargedTo::Subject,
            20,
            kernel_prov(),
        );
        assert!(recs
            .iter()
            .all(|r| r.mode == ReconcileMode::Probe && r.probe_effect_ids.is_empty()));
        assert!(recs.iter().all(|r| r.agreement != Agreement::Agree));
        // A served probe contributes its handle.
        let mut port = Port { deny: false };
        let recs = reconcile_probe(
            &c,
            &[],
            &ReconcileContext::default(),
            &decl(),
            &[("cap/fs_read.probe".into(), Json::Null)],
            &mut port,
            ChargedTo::Subject,
            20,
            kernel_prov(),
        );
        assert!(recs
            .iter()
            .all(|r| r.probe_effect_ids == vec!["probe:e1".to_string()]));
    }

    // AC-R-2.7.2b-6 — outside-gate cap: only critical+irreversible keeps
    // escalate/stop.
    #[test]
    fn outside_gate_cap() {
        assert_eq!(
            cap_outside_gate(Intervention::Stop, SeverityLevel::Medium, false),
            Intervention::FeedBack
        );
        assert_eq!(
            cap_outside_gate(Intervention::Hold, SeverityLevel::Critical, true),
            Intervention::FeedBack
        );
        assert_eq!(
            cap_outside_gate(Intervention::Stop, SeverityLevel::Critical, true),
            Intervention::Stop
        );
        assert_eq!(
            cap_outside_gate(Intervention::Annotate, SeverityLevel::Info, false),
            Intervention::Annotate
        );
    }

    // AC-R-2.7.2b-5 — γ rows are HarnessRule data; a conditioned row
    // without debt is refused.
    #[test]
    fn gamma_from_harness_rules() {
        let rule = hh_hir::records::HarnessRuleRecord {
            rule_id: "rule/gamma.d10".into(),
            trigger: Json::obj([
                ("kind", Json::str("intervention")),
                ("divergence_class", Json::str("d10")),
                ("decision_point", Json::str("verify")),
            ]),
            action: hh_hir::records::RuleAction::RestrictToolSet(vec![]),
            scope: Json::obj([("intervention", Json::str("feed_back"))]),
            conditioned_on: None,
            assumption_debt: None,
        };
        let g = gamma_from_rules(&[rule]).unwrap();
        assert_eq!(g.rows.len(), 1);
        assert_eq!(
            g.rows[0].divergence_class,
            Some(DivergenceClass::NoProgressLoop)
        );
        assert_eq!(g.rows[0].intervention, Intervention::FeedBack);
        // Non-γ rules pass through.
        let other = hh_hir::records::HarnessRuleRecord {
            rule_id: "rule/other".into(),
            trigger: Json::obj([("kind", Json::str("budget"))]),
            action: hh_hir::records::RuleAction::RestrictToolSet(vec![]),
            scope: Json::obj([]),
            conditioned_on: None,
            assumption_debt: None,
        };
        assert!(gamma_from_rules(&[other]).unwrap().rows.is_empty());
        // The disable recipe names the Lab experiment.
        let recipe = disable_gamma_recipe("model/*");
        assert_eq!(recipe.experiment, "interventions_experiment");
        assert_eq!(recipe.kind, "removal_test");
    }

    // AC-R-2.7.2b-7 — the hosted D5-shape gate over end_state.
    #[test]
    fn hosted_gate_end_state() {
        let facts = HostedGateFacts {
            completion_claim_kind: ClaimKind::Achieved,
            completion_claim_agreement: Agreement::Unverifiable,
            end_state_assertions: vec![
                EndStateAssertion {
                    assertion_ref: "assert:tests_pass".into(),
                    declared: Json::Bool(true),
                    measured: Some(Json::Bool(true)),
                    measured_ref: Some("probe:es:1".into()),
                },
                EndStateAssertion {
                    assertion_ref: "assert:build_clean".into(),
                    declared: Json::Bool(true),
                    measured: Some(Json::Bool(false)),
                    measured_ref: Some("probe:es:2".into()),
                },
            ],
            holds_consumed: 0,
            holds_cap: 3,
        };
        let r = evaluate_gate_hosted(&facts);
        assert!(matches!(r.verdict, GateVerdict::Hold { .. }));
        assert!(r.evidence_refs.contains(&"probe:es:2".to_string()));
        // All assertions met → pass.
        let facts = HostedGateFacts {
            end_state_assertions: vec![EndStateAssertion {
                assertion_ref: "assert:tests_pass".into(),
                declared: Json::Bool(true),
                measured: Some(Json::Bool(true)),
                measured_ref: Some("probe:es:1".into()),
            }],
            ..facts
        };
        assert_eq!(evaluate_gate_hosted(&facts).verdict, GateVerdict::Pass);
        // Honest failure: unachievable + unverifiable passes.
        let facts = HostedGateFacts {
            completion_claim_kind: ClaimKind::Unachievable,
            completion_claim_agreement: Agreement::Unverifiable,
            end_state_assertions: vec![],
            holds_consumed: 0,
            holds_cap: 3,
        };
        assert!(evaluate_gate_hosted(&facts).honest_failure);
    }

    // AC-R-2.7.3-12 — session-ABI rows without `model_io` render the
    // judged classes n/a{observability}; interception rows are populated.
    #[test]
    fn na_table() {
        let session_abi: BTreeSet<Observability> = [Observability::EndState].into_iter().collect();
        let na = na_classes(&session_abi, Detector::Judged);
        assert_eq!(na.len(), 6); // every C2 class n/a for a judged detector
        let interception: BTreeSet<Observability> = [
            Observability::EndState,
            Observability::ModelIo,
            Observability::Events,
        ]
        .into_iter()
        .collect();
        assert!(na_classes(&interception, Detector::Judged).is_empty());
        // A deterministic detector over end_state covers D1/D4/D9 only.
        let na = na_classes(&session_abi, Detector::Deterministic);
        assert!(na.contains(&DivergenceClass::CensoredEvidence));
        assert!(na.contains(&DivergenceClass::NoProgressLoop));
        assert!(!na.contains(&DivergenceClass::StaleBelief));
    }

    // ReconciliationRecord validation — judged members are refused on
    // deterministic records.
    #[test]
    fn deterministic_record_carries_no_judged_members() {
        let c = claim(ClaimKind::Assumption, SubjectRef::Run, Json::str("x"));
        let mut r = reconcile_ledger_only(&c, &[], "recon:0", 20, kernel_prov());
        r.calibration_ref = Some("calib/x".into());
        assert!(r.validate().is_err());
    }

    #[test]
    fn criterion_handle_d9() {
        let c = claim(
            ClaimKind::Verified,
            SubjectRef::Criterion("crit:1".into()),
            Json::str("met"),
        );
        let h = AuthoritativeHandle {
            kind: HandleKind::ValidatorVerdict,
            handle_ref: "verdict:v1".into(),
            value: Some(HandleValue::Criterion {
                status: CriterionState::Unmet,
            }),
            authority: AuthorityClass::Kernel,
            produced_at_seq: 8,
        };
        let recs = reconcile_c2(
            &c,
            &[h],
            &ReconcileContext::default(),
            &decl(),
            20,
            kernel_prov(),
        );
        // The C0 fold reads the criterion handle — whatever it decides,
        // the fold must not panic and records stay typed.
        for r in &recs {
            r.validate().unwrap();
        }
    }
}
