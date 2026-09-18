//! `critics` — the independent-critic substrate (spec §5f.4; ADR-0115…0117):
//! `CriticDeclaration`, `IndependenceVector` + the per-use minimums table,
//! `CriticVerdict`, `CalibrationRecord`, `declare_critic`'s refusals, the
//! deterministic-first ordering (`RedundantJudge`), and the programmatic-
//! critic contract (the C0 deterministic critics over ledger/end-state
//! facts — judges and agentic judges are C2).

use std::collections::BTreeSet;

use hh_identity::refs::VersionedRef;
use hh_provenance::authority::AuthorityClass;
use hh_provenance::record::ProvenanceRecord;
use hh_wire::Json;

use crate::evidence::{grounding_of, EvidenceBundle};
use crate::validators::Finding;
use crate::vocab::{
    AdmissionMode, AdversarialClass, CalibrationStatus, CapabilityIndependence, ChargedTo,
    ContextIndependence, CriticKind, CriticUse, Detector, EvidenceMode, Grounding,
    InconclusiveReason, Optimization, ProvenanceIndependence, ReferenceOracle, Site,
    SnapshotIndependence, VerdictType,
};

// ── IndependenceVector (ADR-0116 D1) — declared here since vocab owns sums ──

// `IndependenceVector` is a record (not a closed sum) — declared in this
// module but its axes' sums live in `vocab` (CC7).

/// `IndependenceVector` — mandatory on every declaration; `independence_summary`
/// on every verdict (ADR-0116 D1).
#[derive(Debug, Clone, PartialEq)]
pub struct IndependenceVector {
    /// `different_family | different_snapshot_same_family | same_snapshot`.
    pub snapshot: SnapshotIndependence,
    /// `fresh | shared_transcript | same_context`.
    pub context: ContextIndependence,
    /// `isolated_readonly | shared_readonly | shared_mutable`.
    pub capability: CapabilityIndependence,
    /// `sealed_rubric | lab_rubric | model_authored_rubric`.
    pub provenance: ProvenanceIndependence,
    /// `held_out | in_loop_bounded | in_loop_unbounded`.
    pub optimization: Optimization,
    /// `trusted_weaker | untrusted_monitored | untrusted_unmonitored`.
    pub adversarial: AdversarialClass,
}

impl IndependenceVector {
    /// The canonical `independence_summary` rendering (the verdict metadata).
    pub fn summary(&self) -> String {
        format!(
            "snapshot={} context={} capability={} provenance={} optimization={} adversarial={}",
            self.snapshot.as_str(),
            self.context.as_str(),
            self.capability.as_str(),
            self.provenance.as_str(),
            self.optimization.as_str(),
            self.adversarial.as_str(),
        )
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("adversarial", Json::str(self.adversarial.as_str())),
            ("capability", Json::str(self.capability.as_str())),
            ("context", Json::str(self.context.as_str())),
            ("optimization", Json::str(self.optimization.as_str())),
            ("provenance", Json::str(self.provenance.as_str())),
            ("snapshot", Json::str(self.snapshot.as_str())),
        ])
    }

    /// The fully independent vector (the kernel-critic default).
    pub fn kernel_independent() -> IndependenceVector {
        IndependenceVector {
            snapshot: SnapshotIndependence::DifferentFamily,
            context: ContextIndependence::Fresh,
            capability: CapabilityIndependence::IsolatedReadonly,
            provenance: ProvenanceIndependence::SealedRubric,
            optimization: Optimization::HeldOut,
            adversarial: AdversarialClass::TrustedWeaker,
        }
    }
}

// ── CriticDeclaration (ADR-0115 D1 — `OracleDeclaration ∪ {…}`) ─────────────

/// `CriticDeclaration` — a `Validator` variant declaration (`VariantRecord`,
/// ADR-0023): the `OracleDeclaration` fields plus the critic fields (ADR-0115
/// D1). A beneficiary prompt asking the model to check its own work is a
/// `HarnessRule`, not a critic — it yields no `CriticDeclaration`.
#[derive(Debug, Clone, PartialEq)]
pub struct CriticDeclaration {
    /// The pinned critic ref.
    pub critic_ref: VersionedRef,
    /// The oracle id this critic registers (the `OracleDeclaration` half).
    pub oracle_id: String,
    /// The oracle class (ADR-0047 D1's nine-class sum).
    pub oracle_class: crate::vocab::OracleClass,
    /// Whether the critic's check is deterministic.
    pub deterministic: bool,
    /// The critic kind.
    pub critic_kind: CriticKind,
    /// The evidence modes the bundle may draw on.
    pub evidence_mode: BTreeSet<EvidenceMode>,
    /// The declared admission mode.
    pub admission_mode: AdmissionMode,
    /// The probe capabilities (read-only ∩ parent — `ProbeNotReadOnly` on
    /// anything else; stored as refs + declared read-only flags).
    pub probe_capabilities: Vec<ProbeCapability>,
    /// The independence vector (mandatory).
    pub independence: IndependenceVector,
    /// `instrument | runtime`.
    pub site: Site,
    /// `report | gate`.
    pub use_: CriticUse,
    /// The placement (decision-point ref), when runtime-placed.
    pub placement: Option<String>,
    /// The verdict type.
    pub verdict_type: VerdictType,
    /// Whether the critic is comparative (evaluated in both orders).
    pub comparative: bool,
    /// The rubric ref (`Ref<Text{authority ≥ definition}>` —
    /// `RubricBelowDefinition` below).
    pub rubric_ref: Option<RubricRef>,
    /// The calibration ref.
    pub calibration_ref: Option<String>,
    /// The critics this one is held out from (evolution; C4).
    pub held_out_from: Vec<String>,
    /// The assumption-debt record (mandatory on every critic).
    pub debt: Option<String>,
    /// Who the critic's calls are charged to.
    pub charged_to: ChargedTo,
    /// The calibration status the declaration currently resolves to
    /// (`active | expiring | expired` — gate judges degrade on `expired`).
    pub calibration_status: CalibrationStatus,
    /// Whether deterministic coverage exists at the placement (the
    /// `RedundantJudge` check's input — the deterministic-first ordering).
    pub deterministic_coverage_at_placement: bool,
}

/// A declared probe capability (`read_only ∩ parent` — checked at declare).
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeCapability {
    /// The capability ref.
    pub capability_ref: String,
    /// The declared read-only flag.
    pub read_only: bool,
}

/// The rubric ref + the rubric's minted authority (the
/// `RubricBelowDefinition` input — a model-authored rubric is `delegate`
/// content and cannot define success).
#[derive(Debug, Clone, PartialEq)]
pub struct RubricRef {
    /// The `Ref<Text>` rendering.
    pub text_ref: String,
    /// The rubric's authority.
    pub authority: AuthorityClass,
}

/// `declare_critic`/`consume` refusals (ADR-0115 D1, ADR-0116 D2, ADR-0117 D6 —
/// typed, never a warning).
#[derive(Debug, Clone, PartialEq)]
pub enum CriticError {
    /// The rubric's authority is below `definition` (`RubricBelowDefinition`).
    RubricBelowDefinition {
        /// The rubric's authority.
        authority: AuthorityClass,
    },
    /// A declared probe is not read-only (`ProbeNotReadOnly`).
    ProbeNotReadOnly {
        /// The offending capability.
        capability_ref: String,
    },
    /// A `gate` judge without an active `CalibrationRecord`
    /// (`GateWithoutCalibration` — except `critic_kind = monitor`, veto-only).
    GateWithoutCalibration,
    /// The independence vector is below the per-use minimum
    /// (`IndependenceBelowMinimum(use)`).
    IndependenceBelowMinimum {
        /// The use.
        use_: String,
        /// The violated axis rule.
        axis: String,
    },
    /// `snapshot = same_snapshot` for a non-exploratory use, or
    /// `context = same_context` (same-context critics are not critics —
    /// `JudgeNotIndependent`).
    JudgeNotIndependent {
        /// The violated axis.
        axis: String,
    },
    /// A judge placed where a deterministic check exists (`RedundantJudge` —
    /// a reported defect).
    RedundantJudge,
    /// `grounding` below the use's requirement (`GroundingInsufficient` —
    /// headline and every `use = gate` require `measured`/`reconciled`).
    GroundingInsufficient {
        /// The derived grounding.
        grounding: Grounding,
    },
    /// A `use = gate` verdict surfaced as reported success, or an
    /// `artifact_benefit` produced inside the `held_out_from` closure
    /// (`JudgeLeakedIntoArtifact` / `VerdictMisuse`).
    VerdictMisuse {
        /// The misuse.
        reason: String,
    },
    /// The declaration's `deterministic` flag contradicts `critic_kind`/
    /// `oracle_class`.
    DetectorConflict {
        /// The conflict.
        reason: String,
    },
}

impl std::fmt::Display for CriticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CriticError::RubricBelowDefinition { authority } => {
                write!(f, "RubricBelowDefinition: {authority:?}")
            }
            CriticError::ProbeNotReadOnly { capability_ref } => {
                write!(f, "ProbeNotReadOnly: {capability_ref}")
            }
            CriticError::GateWithoutCalibration => write!(f, "GateWithoutCalibration"),
            CriticError::IndependenceBelowMinimum { use_, axis } => {
                write!(f, "IndependenceBelowMinimum({use_}): {axis}")
            }
            CriticError::JudgeNotIndependent { axis } => {
                write!(f, "JudgeNotIndependent: {axis}")
            }
            CriticError::RedundantJudge => write!(f, "RedundantJudge"),
            CriticError::GroundingInsufficient { grounding } => {
                write!(f, "GroundingInsufficient: {}", grounding.as_str())
            }
            CriticError::VerdictMisuse { reason } => write!(f, "VerdictMisuse: {reason}"),
            CriticError::DetectorConflict { reason } => {
                write!(f, "DetectorConflict: {reason}")
            }
        }
    }
}

impl std::error::Error for CriticError {}

/// `declare_critic` — the declaration contract (ADR-0115 D1, ADR-0116 D1–D4,
/// ADR-0117 D2/D6): per-use independence minimums are checked here and
/// re-checked at `consume`; `same_context` critics are not critics.
pub fn declare_critic(decl: &CriticDeclaration) -> Result<(), CriticError> {
    // Deterministic/kind coherence: model-based kinds are never
    // `deterministic`; a `deterministic` declaration must carry a
    // deterministic oracle class.
    if decl.critic_kind.is_model_based() && decl.deterministic {
        return Err(CriticError::DetectorConflict {
            reason: "model-based critic declared deterministic".into(),
        });
    }
    if decl.deterministic && !decl.oracle_class.is_c0_headline() {
        return Err(CriticError::DetectorConflict {
            reason: "deterministic ⇒ deterministic oracle class".into(),
        });
    }
    // Rubric authority ≥ definition.
    if let Some(r) = &decl.rubric_ref {
        if r.authority < AuthorityClass::Definition {
            return Err(CriticError::RubricBelowDefinition {
                authority: r.authority,
            });
        }
    }
    // Probes are read-only.
    for p in &decl.probe_capabilities {
        if !p.read_only {
            return Err(CriticError::ProbeNotReadOnly {
                capability_ref: p.capability_ref.clone(),
            });
        }
    }
    // `same_context` critics are not critics — refused for every kind.
    if decl.independence.context == ContextIndependence::SameContext {
        return Err(CriticError::JudgeNotIndependent {
            axis: "context = same_context".into(),
        });
    }
    // `same_snapshot` is never admissible for a non-exploratory use (the C0
    // rule: any `gate` use, and any `report` use of a model-based critic —
    // exploratory report critics record the weaker class).
    if decl.independence.snapshot == SnapshotIndependence::SameSnapshot
        && decl.use_ == CriticUse::Gate
    {
        return Err(CriticError::JudgeNotIndependent {
            axis: "snapshot = same_snapshot".into(),
        });
    }
    // The deterministic-first ordering: a model-based critic beside an
    // existing deterministic check at the same placement is `RedundantJudge`.
    if decl.critic_kind.is_model_based() && decl.deterministic_coverage_at_placement {
        return Err(CriticError::RedundantJudge);
    }
    // Gate uses require calibration for model-based critics (except
    // `monitor`, which is veto-only).
    if decl.use_ == CriticUse::Gate
        && decl.critic_kind.is_model_based()
        && decl.critic_kind != CriticKind::Monitor
        && decl.calibration_status != CalibrationStatus::Active
    {
        return Err(CriticError::GateWithoutCalibration);
    }
    check_independence(decl)?;
    Ok(())
}

/// The per-use independence minimums (ADR-0116 D2):
/// - `gate` at `stop`/`verify` — `snapshot ≠ same_snapshot ∧ context = fresh ∧
///   provenance ≠ model_authored_rubric ∧ capability ≠ shared_mutable ∧
///   optimization = in_loop_bounded (or better) ∧ calibration active`.
/// - `gate` at `authorize` — the above `∧ capability = isolated_readonly ∧
///   admission_mode ∈ {principal_only, quarantined_external}`.
/// - `monitor` — `adversarial ∈ {trusted_weaker, untrusted_monitored}`,
///   veto-only.
/// - `report` (judged metric/followed detector) — the snapshot/context/
///   provenance/capability half.
///
/// A `programmatic` critic satisfies the model axes trivially (no model to
/// correlate) but the context/capability axes still bind.
pub fn check_independence(decl: &CriticDeclaration) -> Result<(), CriticError> {
    let v = &decl.independence;
    let model_based = decl.critic_kind.is_model_based();

    // `untrusted_unmonitored` is exploratory only (ADR-0116 D6).
    if decl.critic_kind == CriticKind::Monitor
        && v.adversarial == AdversarialClass::UntrustedUnmonitored
        && decl.use_ == CriticUse::Gate
    {
        return Err(CriticError::IndependenceBelowMinimum {
            use_: "gate/monitor".into(),
            axis: "adversarial = untrusted_unmonitored".into(),
        });
    }

    if decl.use_ != CriticUse::Gate {
        return Ok(());
    }

    // The gate minimum (stop/verify/authorize — the C0 floor).
    if model_based {
        if v.snapshot == SnapshotIndependence::SameSnapshot {
            return Err(CriticError::IndependenceBelowMinimum {
                use_: "gate".into(),
                axis: "snapshot = same_snapshot".into(),
            });
        }
        if v.context != ContextIndependence::Fresh {
            return Err(CriticError::IndependenceBelowMinimum {
                use_: "gate".into(),
                axis: "context ≠ fresh".into(),
            });
        }
        if v.provenance == ProvenanceIndependence::ModelAuthoredRubric {
            return Err(CriticError::IndependenceBelowMinimum {
                use_: "gate".into(),
                axis: "provenance = model_authored_rubric".into(),
            });
        }
    }
    if v.capability == CapabilityIndependence::SharedMutable {
        return Err(CriticError::IndependenceBelowMinimum {
            use_: "gate".into(),
            axis: "capability = shared_mutable".into(),
        });
    }
    if v.optimization == Optimization::InLoopUnbounded {
        return Err(CriticError::IndependenceBelowMinimum {
            use_: "gate".into(),
            axis: "optimization = in_loop_unbounded".into(),
        });
    }
    // `authorize` placements tighten capability + admission.
    if decl.placement.as_deref() == Some("authorize") {
        if v.capability != CapabilityIndependence::IsolatedReadonly {
            return Err(CriticError::IndependenceBelowMinimum {
                use_: "gate/authorize".into(),
                axis: "capability ≠ isolated_readonly".into(),
            });
        }
        if decl.admission_mode == AdmissionMode::AllLabelled {
            return Err(CriticError::IndependenceBelowMinimum {
                use_: "gate/authorize".into(),
                axis: "admission_mode = all_labelled".into(),
            });
        }
    }
    Ok(())
}

/// `consume(verdict, use)` — the consumption admissibility (ADR-0115 D4/D6,
/// ADR-0116 D5): headline and every `use = gate` consumption require
/// `grounding ∈ {measured, reconciled}`; a gate verdict is never the
/// reported success.
pub fn consume(verdict: &CriticVerdict, use_: ConsumeUse) -> Result<(), CriticError> {
    // A gate verdict is never the reported success.
    if use_ == ConsumeUse::Headline && verdict.use_ == CriticUse::Gate {
        return Err(CriticError::VerdictMisuse {
            reason: "a gate verdict is never the reported success".into(),
        });
    }
    // Headline and every gate use require measured/reconciled grounding.
    let needs_grounding = use_ == ConsumeUse::Headline || use_ == ConsumeUse::Gate;
    if needs_grounding
        && !matches!(
            verdict.grounding,
            Grounding::Measured | Grounding::Reconciled
        )
    {
        return Err(CriticError::GroundingInsufficient {
            grounding: verdict.grounding,
        });
    }
    Ok(())
}

/// The `consume` use axis (`report`/`headline` vs `gate` — the report/gate
/// separation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumeUse {
    /// Feed a reported metric.
    Report,
    /// Feed the headline number (the strictest report use).
    Headline,
    /// Feed a gate (authorize / stop / verify / evolution selector).
    Gate,
}

// ── CriticVerdict / CalibrationRecord ───────────────────────────────────────

/// `CriticVerdict` — recorded as `verification.validator.verdict` with the
/// shared extensions (ADR-0115 D5/D8).
#[derive(Debug, Clone, PartialEq)]
pub struct CriticVerdict {
    /// The verdict id.
    pub verdict_id: String,
    /// The critic (pinned `VersionedRef`).
    pub critic_ref: VersionedRef,
    /// The oracle id.
    pub oracle_id: String,
    /// The subject (`{run_id, scope, until_seq}`).
    pub subject: CriticSubject,
    /// The bundle the verdict grounds on.
    pub bundle_id: String,
    /// The critic's declared use.
    pub use_: CriticUse,
    /// The typed verdict value.
    pub verdict: crate::vocab::VerdictValue,
    /// The status (`decided | inconclusive{reason} | oracle_failure{cause}`).
    pub status: crate::vocab::VerdictStatus,
    /// The confidence (ppm).
    pub confidence_ppm: u64,
    /// The derived grounding (`strongest cited evidence_class`; `ungrounded`
    /// ⇒ `oracle_failure`).
    pub grounding: Grounding,
    /// The findings (`evidence_ref` mandatory — uncited findings are dropped
    /// and counted).
    pub findings: Vec<Finding>,
    /// The count of dropped uncited findings.
    pub uncited_findings: u64,
    /// The `independence_summary` the verdict carries (ADR-0116 D1).
    pub independence_summary: String,
    /// The detector class.
    pub detector: Detector,
    /// The calibration ref, when the critic is calibrated.
    pub calibration_ref: Option<String>,
    /// Who the call was charged to (`dimension = evaluator_calls`).
    pub charged_to: ChargedTo,
    /// The trigger reason, when the critic was rule-invoked.
    pub trigger_reason: Option<String>,
    /// The verdict's provenance (`kernel` deterministic / `delegate` judged).
    pub provenance: ProvenanceRecord,
    /// The seq the verdict was produced at.
    pub at_seq: u64,
}

/// `CriticSubject` — `{run_id, scope, until_seq}`.
#[derive(Debug, Clone, PartialEq)]
pub struct CriticSubject {
    /// The run.
    pub run_id: String,
    /// The scope selector.
    pub scope: String,
    /// The ledger horizon.
    pub until_seq: u64,
}

impl CriticVerdict {
    /// Drop uncited findings and count them (ADR-0115 D5 — the verdict
    /// stands; `uncited_finding_rate` feeds calibration).
    pub fn prune_uncited(&mut self, bundle: &EvidenceBundle) {
        let cited: BTreeSet<String> = bundle
            .items
            .iter()
            .map(|i| i.item_ref.clone())
            .chain(bundle.handles.iter().map(|h| h.handle_ref.clone()))
            .collect();
        let before = self.findings.len();
        self.findings
            .retain(|f| f.evidence_ref.as_ref().is_some_and(|r| cited.contains(r)));
        self.uncited_findings += (before - self.findings.len()) as u64;
    }

    /// The verdict's authority contract (deterministic ⇒ `kernel`; judged ⇒
    /// `delegate` — a critic verdict never confers authority).
    pub fn validate(&self) -> Result<(), CriticError> {
        let expected = crate::validators::Verdict::expected_authority(self.detector);
        if self.provenance.authority != expected {
            return Err(CriticError::DetectorConflict {
                reason: format!(
                    "{:?} verdict minted at {expected:?} expected, got {:?}",
                    self.detector, self.provenance.authority
                ),
            });
        }
        Ok(())
    }
}

/// `CalibrationRecord` — the first-class, content-addressed calibration
/// record (ADR-0117 D1; the C0 schema — the Lab runs that produce it are
/// Stage 3/4).
#[derive(Debug, Clone, PartialEq)]
pub struct CalibrationRecord {
    /// The calibration id.
    pub calibration_id: String,
    /// The critic `version_id` calibrated.
    pub critic_ref: String,
    /// The judge snapshot fingerprint.
    pub judge_snapshot_fingerprint: String,
    /// The labelled set (content-addressed; `task_split_hash` predates any
    /// gate use).
    pub labelled_set_ref: String,
    /// Samples per verdict class.
    pub n_per_verdict_class: u64,
    /// The reference oracle (deterministic or `human_panel` — never a
    /// judge; `JudgeAsReference` refuses otherwise).
    pub reference_oracle: ReferenceOracle,
    /// The agreement statistic (`{statistic, value_ppm, interval, method}`).
    pub agreement: CalibrationAgreement,
    /// The error rates (`fn_rate`, `fp_rate` per class, `uncited_finding_rate`
    /// — ppm).
    pub error_rates: Json,
    /// The position-consistency floor (ppm).
    pub position_consistency_ppm: u64,
    /// The self-consistency measure (ppm), when measured.
    pub self_consistency_ppm: Option<u64>,
    /// The self-preference check, when run (mandatory for
    /// `different_snapshot_same_family`).
    pub self_preference_check: Option<Json>,
    /// The evidence-ablation delta (ppm) — mandatory for gate judges.
    pub evidence_ablation_ppm: Option<u64>,
    /// When the calibration ran.
    pub calibrated_at: u64,
    /// The expiry condition refs (`snapshot fingerprint changed | rubric
    /// version changed | task family changed | age > recalibration_period`).
    pub expiry: Vec<String>,
    /// `active | expiring | expired`.
    pub status: CalibrationStatus,
}

/// `CalibrationAgreement` — `{statistic, value_ppm, interval, method}`.
#[derive(Debug, Clone, PartialEq)]
pub struct CalibrationAgreement {
    /// `cohen_kappa | f1_per_class | spearman | mcc`.
    pub statistic: String,
    /// The agreement value (ppm).
    pub value_ppm: u64,
    /// The interval rendering.
    pub interval: String,
    /// The method.
    pub method: String,
}

// `reference_oracle ∈ {executable, end_state, trace_predicate, human_panel}`
// is a closed sum — `judge` is unrepresentable by construction
// (`JudgeAsReference` cannot be produced, ADR-0117 D1).

// ── Programmatic critics (C0 deterministic critics) ─────────────────────────

/// `ProgrammaticCritic` — the C0 programmatic-critic contract: a deterministic
/// `Validator` variant that renders a `CriticVerdict` over a kernel-built
/// `EvidenceBundle` (ledger/end-state facts only — judges are C2). Pure in
/// `(bundle, critic version_id)`; verdicts are `kernel`-origin.
pub trait ProgrammaticCritic {
    /// The critic's pinned declaration.
    fn declaration(&self) -> &CriticDeclaration;
    /// Evaluate the bundle — a `CriticVerdict` with `detector = deterministic`,
    /// `grounding` derived from the cited items, `independence_summary` from
    /// the declaration.
    fn evaluate(
        &self,
        bundle: &EvidenceBundle,
        subject: &CriticSubject,
        provenance: ProvenanceRecord,
        at_seq: u64,
    ) -> CriticVerdict;
}

/// `ReconciliationAgreementCritic` — the reference C0 programmatic critic: the
/// `divergence_profile`/`claim_state_agreement` fold over a bundle of
/// `verification.claim.reconciled` items. Deterministic; `use = report`;
/// never a gate cause by itself at C0 (its verdict feeds the declared
/// grounding metrics).
pub struct ReconciliationAgreementCritic {
    /// The critic's declaration.
    pub declaration: CriticDeclaration,
}

/// The `agreement` fold input — the reconciled agreements in scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AgreementCounts {
    /// `agree` count.
    pub agree: u64,
    /// `diverge` count.
    pub diverge: u64,
    /// `unverifiable` count (excluded from the rate, counted beside it).
    pub unverifiable: u64,
    /// `stale` count (excluded from the rate, counted beside it).
    pub stale: u64,
}

impl AgreementCounts {
    /// `claim_state_agreement = agree ÷ (agree + diverge)` in ppm — `None`
    /// (`n/a`) when the denominator is empty (never 0, never a proxy).
    pub fn claim_state_agreement_ppm(&self) -> Option<u64> {
        let denom = self.agree + self.diverge;
        if denom == 0 {
            return None;
        }
        Some(self.agree * 1_000_000 / denom)
    }

    /// `execution_alignment_failure_rate` input — whether ≥ 1 completion-claim
    /// divergence in `{D2, D3, D5, D6}` fired.
    pub fn has_c0_divergence(&self) -> bool {
        self.diverge > 0
    }
}

impl ProgrammaticCritic for ReconciliationAgreementCritic {
    fn declaration(&self) -> &CriticDeclaration {
        &self.declaration
    }

    fn evaluate(
        &self,
        bundle: &EvidenceBundle,
        subject: &CriticSubject,
        provenance: ProvenanceRecord,
        at_seq: u64,
    ) -> CriticVerdict {
        let grounding = grounding_of(&bundle.items);
        let (status, verdict) = if grounding == Grounding::Ungrounded {
            (
                crate::vocab::VerdictStatus::Inconclusive(InconclusiveReason::MissingEvidence),
                crate::vocab::VerdictValue::ThreeValued(crate::vocab::ThreeValued::Inconclusive),
            )
        } else {
            (
                crate::vocab::VerdictStatus::Decided,
                crate::vocab::VerdictValue::Bool(true),
            )
        };
        CriticVerdict {
            verdict_id: format!("critver:{}:{}", self.declaration.oracle_id, subject.run_id),
            critic_ref: self.declaration.critic_ref.clone(),
            oracle_id: self.declaration.oracle_id.clone(),
            subject: subject.clone(),
            bundle_id: bundle.bundle_id.clone(),
            use_: self.declaration.use_,
            verdict,
            status,
            confidence_ppm: 1_000_000,
            grounding,
            findings: vec![],
            uncited_findings: 0,
            independence_summary: self.declaration.independence.summary(),
            detector: Detector::Deterministic,
            calibration_ref: self.declaration.calibration_ref.clone(),
            charged_to: self.declaration.charged_to,
            trigger_reason: None,
            provenance,
            at_seq,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::SeverityLevel;
    use hh_identity::kinds::RecordKind;
    use hh_provenance::authority::PersistenceScope;
    use hh_provenance::origin::Origin;

    fn prov(authority_scope: PersistenceScope) -> ProvenanceRecord {
        ProvenanceRecord::minted(Origin::kernel("hir/kernel/critic"), authority_scope, 1)
    }

    fn critic_ref() -> VersionedRef {
        VersionedRef::pinned(
            RecordKind::Validator,
            "sha256:c0",
            prov(PersistenceScope::Definition),
        )
    }

    fn decl() -> CriticDeclaration {
        CriticDeclaration {
            critic_ref: critic_ref(),
            oracle_id: "oracle:agreement".into(),
            oracle_class: crate::vocab::OracleClass::TracePredicate,
            deterministic: true,
            critic_kind: CriticKind::Programmatic,
            evidence_mode: [EvidenceMode::LedgerView].into_iter().collect(),
            admission_mode: AdmissionMode::PrincipalOnly,
            probe_capabilities: vec![],
            independence: IndependenceVector::kernel_independent(),
            site: Site::Instrument,
            use_: CriticUse::Report,
            placement: None,
            verdict_type: VerdictType::Bool,
            comparative: false,
            rubric_ref: None,
            calibration_ref: None,
            held_out_from: vec![],
            debt: Some("debt:c".into()),
            charged_to: ChargedTo::Instrument,
            calibration_status: CalibrationStatus::Active,
            deterministic_coverage_at_placement: false,
        }
    }

    #[test]
    fn same_context_critics_are_not_critics() {
        let mut d = decl();
        d.independence.context = ContextIndependence::SameContext;
        assert!(matches!(
            declare_critic(&d),
            Err(CriticError::JudgeNotIndependent { .. })
        ));
    }

    #[test]
    fn rubric_below_definition_is_refused() {
        let mut d = decl();
        d.rubric_ref = Some(RubricRef {
            text_ref: "text:r".into(),
            authority: AuthorityClass::Delegate,
        });
        assert_eq!(
            declare_critic(&d),
            Err(CriticError::RubricBelowDefinition {
                authority: AuthorityClass::Delegate,
            })
        );
        d.rubric_ref = Some(RubricRef {
            text_ref: "text:r".into(),
            authority: AuthorityClass::Definition,
        });
        declare_critic(&d).unwrap();
    }

    #[test]
    fn non_readonly_probe_is_refused() {
        let mut d = decl();
        d.probe_capabilities.push(ProbeCapability {
            capability_ref: "cap:fs_write".into(),
            read_only: false,
        });
        assert!(matches!(
            declare_critic(&d),
            Err(CriticError::ProbeNotReadOnly { .. })
        ));
    }

    #[test]
    fn redundant_judge_beside_deterministic_check() {
        // AC-R-2.7.3-13: a judge where a deterministic check exists ⇒ refused.
        let mut d = decl();
        d.critic_kind = CriticKind::Judge;
        d.deterministic = false;
        d.oracle_class = crate::vocab::OracleClass::Judge;
        d.deterministic_coverage_at_placement = true;
        assert_eq!(declare_critic(&d), Err(CriticError::RedundantJudge));
    }

    #[test]
    fn gate_judge_requires_calibration_and_minimums() {
        let mut d = decl();
        d.critic_kind = CriticKind::Judge;
        d.deterministic = false;
        d.oracle_class = crate::vocab::OracleClass::Judge;
        d.use_ = CriticUse::Gate;
        d.calibration_status = CalibrationStatus::Expired;
        assert_eq!(declare_critic(&d), Err(CriticError::GateWithoutCalibration));
        // Fresh context is required for a gate judge.
        d.calibration_status = CalibrationStatus::Active;
        d.independence.context = ContextIndependence::SharedTranscript;
        assert!(matches!(
            declare_critic(&d),
            Err(CriticError::IndependenceBelowMinimum { .. })
        ));
        d.independence.context = ContextIndependence::Fresh;
        d.independence.optimization = Optimization::InLoopBounded;
        declare_critic(&d).unwrap();
    }

    #[test]
    fn authorize_placement_tightens_capability_and_admission() {
        let mut d = decl();
        d.use_ = CriticUse::Gate;
        d.placement = Some("authorize".into());
        d.independence.capability = CapabilityIndependence::SharedReadonly;
        assert!(matches!(
            declare_critic(&d),
            Err(CriticError::IndependenceBelowMinimum { .. })
        ));
        d.independence.capability = CapabilityIndependence::IsolatedReadonly;
        d.admission_mode = AdmissionMode::AllLabelled;
        assert!(matches!(
            declare_critic(&d),
            Err(CriticError::IndependenceBelowMinimum { .. })
        ));
        d.admission_mode = AdmissionMode::PrincipalOnly;
        declare_critic(&d).unwrap();
    }

    #[test]
    fn consume_enforces_grounding_and_gate_report_separation() {
        // AC-R-2.7.3-1: claimed-only grounding is inadmissible for gate/headline.
        let mut v = CriticVerdict {
            verdict_id: "v:1".into(),
            critic_ref: critic_ref(),
            oracle_id: "o".into(),
            subject: CriticSubject {
                run_id: "r".into(),
                scope: "run".into(),
                until_seq: 9,
            },
            bundle_id: "b".into(),
            use_: CriticUse::Report,
            verdict: crate::vocab::VerdictValue::Bool(true),
            status: crate::vocab::VerdictStatus::Decided,
            confidence_ppm: 1_000_000,
            grounding: Grounding::Claimed,
            findings: vec![],
            uncited_findings: 0,
            independence_summary: "s".into(),
            detector: Detector::Judged,
            calibration_ref: None,
            charged_to: ChargedTo::Instrument,
            trigger_reason: None,
            provenance: ProvenanceRecord::minted(
                Origin::model("m", "r", "call"),
                PersistenceScope::Run,
                1,
            ),
            at_seq: 9,
        };
        assert!(matches!(
            consume(&v, ConsumeUse::Gate),
            Err(CriticError::GroundingInsufficient { .. })
        ));
        v.grounding = Grounding::Measured;
        assert!(consume(&v, ConsumeUse::Gate).is_ok());
        // Gate verdicts are never reported success.
        v.use_ = CriticUse::Gate;
        assert!(matches!(
            consume(&v, ConsumeUse::Headline),
            Err(CriticError::VerdictMisuse { .. })
        ));
    }

    #[test]
    fn uncited_findings_are_dropped_and_counted() {
        let bundle = EvidenceBundle {
            bundle_id: "b".into(),
            handles: vec![],
            items: vec![crate::evidence::EvidenceItem {
                item_ref: "evt:1".into(),
                evidence_class: crate::vocab::EvidenceClass::Measured,
                authority: AuthorityClass::Kernel,
                provenance: prov(PersistenceScope::Run),
                label: "fact".into(),
                bytes_or_view_hash: "h".into(),
                truncated: false,
            }],
            omitted: vec![],
            task_contract_ref: None,
            reference_ref: None,
            inputs_digest: "d".into(),
        };
        let mut v = CriticVerdict {
            verdict_id: "v:1".into(),
            critic_ref: critic_ref(),
            oracle_id: "o".into(),
            subject: CriticSubject {
                run_id: "r".into(),
                scope: "run".into(),
                until_seq: 9,
            },
            bundle_id: "b".into(),
            use_: CriticUse::Report,
            verdict: crate::vocab::VerdictValue::Bool(true),
            status: crate::vocab::VerdictStatus::Decided,
            confidence_ppm: 1_000_000,
            grounding: Grounding::Measured,
            findings: vec![
                crate::validators::Finding {
                    code: "f1".into(),
                    severity: SeverityLevel::Low,
                    location: None,
                    message: "cited".into(),
                    evidence_ref: Some("evt:1".into()),
                },
                crate::validators::Finding {
                    code: "f2".into(),
                    severity: SeverityLevel::Low,
                    location: None,
                    message: "hallucinated".into(),
                    evidence_ref: Some("evt:999".into()),
                },
            ],
            uncited_findings: 0,
            independence_summary: "s".into(),
            detector: Detector::Deterministic,
            calibration_ref: None,
            charged_to: ChargedTo::Instrument,
            trigger_reason: None,
            provenance: prov(PersistenceScope::Run),
            at_seq: 9,
        };
        v.prune_uncited(&bundle);
        assert_eq!(v.findings.len(), 1);
        assert_eq!(v.uncited_findings, 1);
    }

    #[test]
    fn agreement_counts_render_the_declared_rates() {
        let c = AgreementCounts {
            agree: 3,
            diverge: 1,
            unverifiable: 2,
            stale: 1,
        };
        assert_eq!(c.claim_state_agreement_ppm(), Some(750_000));
        assert!(c.has_c0_divergence());
        assert_eq!(AgreementCounts::default().claim_state_agreement_ppm(), None);
    }

    #[test]
    fn programmatic_critic_renders_kernel_verdicts() {
        let critic = ReconciliationAgreementCritic {
            declaration: decl(),
        };
        let bundle = EvidenceBundle {
            bundle_id: "b".into(),
            handles: vec![],
            items: vec![crate::evidence::EvidenceItem {
                item_ref: "evt:1".into(),
                evidence_class: crate::vocab::EvidenceClass::Reconciled,
                authority: AuthorityClass::Kernel,
                provenance: prov(PersistenceScope::Run),
                label: "fact".into(),
                bytes_or_view_hash: "h".into(),
                truncated: false,
            }],
            omitted: vec![],
            task_contract_ref: None,
            reference_ref: None,
            inputs_digest: "d".into(),
        };
        let v = critic.evaluate(
            &bundle,
            &CriticSubject {
                run_id: "r".into(),
                scope: "run".into(),
                until_seq: 9,
            },
            ProvenanceRecord::kernel("hir/kernel/critic", 9),
            9,
        );
        assert_eq!(v.detector, Detector::Deterministic);
        assert_eq!(v.grounding, Grounding::Reconciled);
        assert!(v.independence_summary.contains("snapshot=different_family"));
        v.validate().unwrap();
    }
}
