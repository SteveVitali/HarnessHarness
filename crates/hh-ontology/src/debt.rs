//! The **canonical assumption-debt vocabulary** (R-2.9.6⁰ᵃ, §5h.6; ADR-0197/0198):
//! `DebtHomes/1` — the closed 17-home inventory — plus the shared `AssumptionDebtRecord/1`
//! vocabulary every home carries: `DebtClass`, `DeficiencyClass/1`, `DebtStatus`,
//! `ExpiryKind`/`ExpiryCondition`, `EvidenceRef`/`EvidenceGrade`, `DebtScope`,
//! `HypothesisTyped`, `RemovalTest`/`RemovalVerdict`, `OwnerRef`, `DebtRef`, `DebtPolicy`,
//! `Revalidation`, and the pure `required_fields`/`removal_test_instantiates` predicates.
//!
//! CF-049: **the vocabulary differs across homes; the field shape does not.** One schema
//! source (CC7) — the types here are plain-data members (no `Text` leaves) so every home
//! can carry them; the HIR home's record adds the `hypothesis: Text` leaf.
//!
//! `evidence_grade` is **derived, never stored** (ADR-0197): the grade is computed from
//! the instrument refs the `evidence_refs` carry — a stored grade would drift from the
//! refs it claims to summarize.

use std::collections::{BTreeMap, BTreeSet};

use hh_wire::Json;

/// `DebtStatus` — the canonical four-value lifecycle sum (§5h.6 §3):
/// `active → expiring → expired → retired`. `retired` is the only exit; expired
/// records are retained and annotated, never silently deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DebtStatus {
    /// `active` — the debt is recorded and its expiry is not in view.
    Active,
    /// `expiring` — the expiry condition is inside `warn_within`.
    Expiring,
    /// `expired` — the expiry condition has fired; the record is retained and
    /// annotated (use emits `lifecycle.debt.expired_used`).
    Expired,
    /// `retired` — the debt is discharged by evidence + human sealing
    /// (`RetirementRecord`).
    Retired,
}

impl DebtStatus {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            DebtStatus::Active => "active",
            DebtStatus::Expiring => "expiring",
            DebtStatus::Expired => "expired",
            DebtStatus::Retired => "retired",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<DebtStatus> {
        match s {
            "active" => Some(DebtStatus::Active),
            "expiring" => Some(DebtStatus::Expiring),
            "expired" => Some(DebtStatus::Expired),
            "retired" => Some(DebtStatus::Retired),
            _ => None,
        }
    }

    /// The legacy interim spellings (pre-`/1` `hh_hir::DebtStatus` /
    /// `hh_compiler::profile::DebtStatus`): `open` → `active`, `discharged` →
    /// `retired`, `violated` → `expired`. Decoding tolerates them so landed
    /// bodies keep decoding (CC8); emission is always canonical.
    pub fn parse_legacy(s: &str) -> Option<DebtStatus> {
        match s {
            "open" => Some(DebtStatus::Active),
            "discharged" => Some(DebtStatus::Retired),
            "violated" => Some(DebtStatus::Expired),
            _ => DebtStatus::parse(s),
        }
    }
}

/// `ExpiryKind` — the closed expiry-condition sum (§5h.6 §3): the trigger that
/// moves a debt `active → expiring → expired`. `experiment_ref` is bound
/// outside `ExpiryCondition.value` (the `expiry{condition, params{…}}` additive
/// record carries the per-kind operands).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExpiryKind {
    /// `model_version_change` — a model-version bound; `value`/`until` carries
    /// the version operand.
    ModelVersionChange,
    /// `date` — a wall-clock bound; `value`/`until` carries the date.
    Date,
    /// `probe_failure` — a probe/fingerprint regression fires it.
    ProbeFailure,
    /// `evidence_refresh_due` — the evidence refs are past `evidence_max_age`.
    EvidenceRefreshDue,
    /// `experiment_ref` — the debt expires when the bound experiment settles.
    ExperimentRef,
}

impl ExpiryKind {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            ExpiryKind::ModelVersionChange => "model_version_change",
            ExpiryKind::Date => "date",
            ExpiryKind::ProbeFailure => "probe_failure",
            ExpiryKind::EvidenceRefreshDue => "evidence_refresh_due",
            ExpiryKind::ExperimentRef => "experiment_ref",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<ExpiryKind> {
        match s {
            "model_version_change" => Some(ExpiryKind::ModelVersionChange),
            "date" => Some(ExpiryKind::Date),
            "probe_failure" => Some(ExpiryKind::ProbeFailure),
            "evidence_refresh_due" => Some(ExpiryKind::EvidenceRefreshDue),
            "experiment_ref" => Some(ExpiryKind::ExperimentRef),
            _ => None,
        }
    }
}

/// `ExpiryCondition{kind, value?}` — the debt's expiry trigger in the field
/// shape the landed profile/HIR codecs already emit (object form). A bare
/// spelling (`"expiry_condition": "date"`) decodes as `{kind, value: null}` for
/// back-compat; emission is always the object form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpiryCondition {
    /// The closed kind sum.
    pub kind: ExpiryKind,
    /// The kind's operand (a date string, version bound, etc.) — bound outside
    /// `kind` so the closed sum stays closed.
    pub value: Option<String>,
}

impl ExpiryCondition {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("kind".into(), Json::str(self.kind.name()));
        if let Some(v) = &self.value {
            m.insert("value".into(), Json::str(v.clone()));
        }
        Json::Obj(m)
    }
}

/// `ExpiryParams` — the per-kind operands of the additive `expiry{condition,
/// params}` member (§5h.6 §3 `/1`): `until?`, `evidence_max_age?`,
/// `dependency_capabilities[]?`, `design_ref?`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExpiryParams {
    /// `until` — the ms-epoch bound a `date`/`model_version_change` expiry
    /// names (the `InsufficientRunway` check compares `until − created_at`
    /// against `DebtPolicy.min_runway`).
    pub until: Option<u64>,
    /// `evidence_max_age` — the bound on evidence-ref age.
    pub evidence_max_age_ms: Option<u64>,
    /// `dependency_capabilities[]` — the capabilities the expiry depends on.
    pub dependency_capabilities: Vec<String>,
    /// `design_ref` — the experiment/design the `experiment_ref` expiry binds.
    pub design_ref: Option<String>,
}

/// `DebtExpiry{condition, params}` — the additive `/1` expiry member
/// (§5h.6 §3): the typed, parameterized form of `expiry_condition`. A record
/// may carry `expiry_condition` alone (the ratified member); `expiry` is the
/// additive richer form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebtExpiry {
    /// The closed condition kind.
    pub condition: ExpiryKind,
    /// The per-kind operands.
    pub params: ExpiryParams,
}

/// `DebtClass` — the closed debt-class sum (§5h.6 §3): what kind of claim the
/// debt carries. Defaulted per home (the `DebtHomes/1` row's `debt_class`
/// column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DebtClass {
    /// `model_conditioned` — the claim holds only for a model scope; needs
    /// `scope.model_selectors`.
    ModelConditioned,
    /// `cross_model` — the claim is asserted across models.
    CrossModel,
    /// `empirical` — the claim is backed by observed evidence.
    Empirical,
    /// `hypothesized` — the claim is hypothesized; the hypothesis field is
    /// mandatory.
    Hypothesized,
    /// `unclassified` — no class asserted (the interim default).
    Unclassified,
}

impl DebtClass {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            DebtClass::ModelConditioned => "model_conditioned",
            DebtClass::CrossModel => "cross_model",
            DebtClass::Empirical => "empirical",
            DebtClass::Hypothesized => "hypothesized",
            DebtClass::Unclassified => "unclassified",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<DebtClass> {
        match s {
            "model_conditioned" => Some(DebtClass::ModelConditioned),
            "cross_model" => Some(DebtClass::CrossModel),
            "empirical" => Some(DebtClass::Empirical),
            "hypothesized" => Some(DebtClass::Hypothesized),
            "unclassified" => Some(DebtClass::Unclassified),
            _ => None,
        }
    }
}

/// `DeficiencyClass/1` — the closed deficiency-class sum (§5h.6 §3): what the
/// harness deficit the debt tracks *is*. `unknown` carries an inline
/// explanation (the spec-debt register spells it `unknown{"…"}`); `unknown` is
/// admissible only with `evidence_grade = hypothesized`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeficiencyClass {
    /// `premature_stop` — the run stopped before the task was done.
    PrematureStop,
    /// `format_noncompliance` — output format non-compliance.
    FormatNoncompliance,
    /// `tool_shape_mismatch` — the tool surface didn't match.
    ToolShapeMismatch,
    /// `name_misalignment` — the harness's names confused the model.
    NameMisalignment,
    /// `schema_dialect_gap` — a schema/dialect mismatch.
    SchemaDialectGap,
    /// `context_overflow_handling` — context-window overflow handling.
    ContextOverflowHandling,
    /// `loop_or_repetition` — degenerate looping/repetition.
    LoopOrRepetition,
    /// `instruction_following_gap` — instruction-following failure.
    InstructionFollowingGap,
    /// `claim_grounding_gap` — ungrounded claims.
    ClaimGroundingGap,
    /// `reasoning_replay_incompatibility` — reasoning-replay mismatch.
    ReasoningReplayIncompatibility,
    /// `latency_or_cost_profile` — latency/cost profile deficiency.
    LatencyOrCostProfile,
    /// `unknown{…}` — an unclassified deficiency; the inline explanation is
    /// mandatory. Admissible only with `evidence_grade = hypothesized`.
    Unknown(String),
}

impl DeficiencyClass {
    /// The canonical spelling (the `unknown` member's spelling is `"unknown"`;
    /// its payload rides the object member).
    pub fn name(&self) -> &'static str {
        match self {
            DeficiencyClass::PrematureStop => "premature_stop",
            DeficiencyClass::FormatNoncompliance => "format_noncompliance",
            DeficiencyClass::ToolShapeMismatch => "tool_shape_mismatch",
            DeficiencyClass::NameMisalignment => "name_misalignment",
            DeficiencyClass::SchemaDialectGap => "schema_dialect_gap",
            DeficiencyClass::ContextOverflowHandling => "context_overflow_handling",
            DeficiencyClass::LoopOrRepetition => "loop_or_repetition",
            DeficiencyClass::InstructionFollowingGap => "instruction_following_gap",
            DeficiencyClass::ClaimGroundingGap => "claim_grounding_gap",
            DeficiencyClass::ReasoningReplayIncompatibility => "reasoning_replay_incompatibility",
            DeficiencyClass::LatencyOrCostProfile => "latency_or_cost_profile",
            DeficiencyClass::Unknown(_) => "unknown",
        }
    }

    /// Parse the canonical spelling (the `unknown` payload is decoded by the
    /// caller — `parse` accepts only the bare spellings; `unknown` is decoded
    /// from `{unknown: "…"}`).
    pub fn parse(s: &str) -> Option<DeficiencyClass> {
        match s {
            "premature_stop" => Some(DeficiencyClass::PrematureStop),
            "format_noncompliance" => Some(DeficiencyClass::FormatNoncompliance),
            "tool_shape_mismatch" => Some(DeficiencyClass::ToolShapeMismatch),
            "name_misalignment" => Some(DeficiencyClass::NameMisalignment),
            "schema_dialect_gap" => Some(DeficiencyClass::SchemaDialectGap),
            "context_overflow_handling" => Some(DeficiencyClass::ContextOverflowHandling),
            "loop_or_repetition" => Some(DeficiencyClass::LoopOrRepetition),
            "instruction_following_gap" => Some(DeficiencyClass::InstructionFollowingGap),
            "claim_grounding_gap" => Some(DeficiencyClass::ClaimGroundingGap),
            "reasoning_replay_incompatibility" => {
                Some(DeficiencyClass::ReasoningReplayIncompatibility)
            }
            "latency_or_cost_profile" => Some(DeficiencyClass::LatencyOrCostProfile),
            _ => None,
        }
    }

    /// The canonical JSON — a bare spelling for the closed members;
    /// `{unknown: "…"}` for the open arm.
    pub fn to_json(&self) -> Json {
        match self {
            DeficiencyClass::Unknown(text) => Json::obj([("unknown", Json::str(text.clone()))]),
            _ => Json::str(self.name()),
        }
    }
}

/// `EvidenceKind` — the closed evidence-reference kind sum (§5h.6 §3): what
/// kind of instrument/artifact produced the evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EvidenceKind {
    /// `source` — a source-level pointer (spec text, code, doc).
    Source,
    /// `inspection` — human inspection/review.
    Inspection,
    /// `probe_run` — a probe/fingerprint run.
    ProbeRun,
    /// `model_probe` — a model-behavior probe.
    ModelProbe,
    /// `experiment` — an experiment report.
    Experiment,
    /// `conformance_report` — a conformance report.
    ConformanceReport,
    /// `challenge_result` — a challenge/red-team result.
    ChallengeResult,
    /// `attestation` — an attestation.
    Attestation,
    /// `experiment_report` — an experiment report (legacy spelling of the
    /// bound experiment evidence).
    ExperimentReport,
    /// `rule_outcome` — a rule-outcome observation.
    RuleOutcome,
}

impl EvidenceKind {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            EvidenceKind::Source => "source",
            EvidenceKind::Inspection => "inspection",
            EvidenceKind::ProbeRun => "probe_run",
            EvidenceKind::ModelProbe => "model_probe",
            EvidenceKind::Experiment => "experiment",
            EvidenceKind::ConformanceReport => "conformance_report",
            EvidenceKind::ChallengeResult => "challenge_result",
            EvidenceKind::Attestation => "attestation",
            EvidenceKind::ExperimentReport => "experiment_report",
            EvidenceKind::RuleOutcome => "rule_outcome",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<EvidenceKind> {
        match s {
            "source" => Some(EvidenceKind::Source),
            "inspection" => Some(EvidenceKind::Inspection),
            "probe_run" => Some(EvidenceKind::ProbeRun),
            "model_probe" => Some(EvidenceKind::ModelProbe),
            "experiment" => Some(EvidenceKind::Experiment),
            "conformance_report" => Some(EvidenceKind::ConformanceReport),
            "challenge_result" => Some(EvidenceKind::ChallengeResult),
            "attestation" => Some(EvidenceKind::Attestation),
            "experiment_report" => Some(EvidenceKind::ExperimentReport),
            "rule_outcome" => Some(EvidenceKind::RuleOutcome),
            _ => None,
        }
    }

    /// Whether the kind counts as a real *instrument* ref for the
    /// `evidence_grade` derivation (§5h.6 §3): experiment reports, probes,
    /// conformance reports and challenge results are instruments; `source` /
    /// `inspection` / `attestation` / `rule_outcome` are not.
    pub fn is_instrument(self) -> bool {
        matches!(
            self,
            EvidenceKind::ProbeRun
                | EvidenceKind::ModelProbe
                | EvidenceKind::Experiment
                | EvidenceKind::ExperimentReport
                | EvidenceKind::ConformanceReport
                | EvidenceKind::ChallengeResult
        )
    }
}

/// `EvidenceRef{kind, ref, observed_at?, tier?, provisional?}` — a typed
/// evidence reference (§5h.6 §3). A bare-string `evidence_refs` member decodes
/// as `{kind: source, ref, observed_at: null}` (legacy bodies; CC8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRef {
    /// The evidence kind.
    pub kind: EvidenceKind,
    /// The reference (a report ref, content address, or pointer string).
    pub reference: String,
    /// When the evidence was observed (transaction time), if known.
    pub observed_at: Option<u64>,
    /// The evidence tier, when asserted.
    pub tier: Option<String>,
    /// Whether the evidence is provisional.
    pub provisional: bool,
}

impl EvidenceRef {
    /// A legacy bare-string ref (`{kind: source, ref}` — no observation time).
    pub fn legacy(reference: impl Into<String>) -> EvidenceRef {
        EvidenceRef {
            kind: EvidenceKind::Source,
            reference: reference.into(),
            observed_at: None,
            tier: None,
            provisional: false,
        }
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("kind".into(), Json::str(self.kind.name()));
        m.insert("ref".into(), Json::str(self.reference.clone()));
        if let Some(t) = self.observed_at {
            m.insert("observed_at".into(), Json::Int(t as i64));
        }
        if let Some(t) = &self.tier {
            m.insert("tier".into(), Json::str(t.clone()));
        }
        if self.provisional {
            m.insert("provisional".into(), Json::Bool(true));
        }
        Json::Obj(m)
    }
}

/// `EvidenceGrade` — the *derived* evidence grade (§5h.6 §3; ADR-0197). Never
/// stored: [`evidence_grade`] computes it from the `EvidenceRef` kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EvidenceGrade {
    /// `hypothesized` — no instrument evidence, or a single `rule_outcome` ref.
    Hypothesized,
    /// `evidenced` — ≥ 1 real instrument ref.
    Evidenced,
    /// `confirmed` — ≥ 2 instruments from different families.
    Confirmed,
}

impl EvidenceGrade {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            EvidenceGrade::Hypothesized => "hypothesized",
            EvidenceGrade::Evidenced => "evidenced",
            EvidenceGrade::Confirmed => "confirmed",
        }
    }
}

/// Derive the evidence grade (ADR-0197 — derived, never stored):
/// `confirmed` iff ≥ 2 instrument refs of *different* `EvidenceKind`s;
/// `evidenced` iff ≥ 1 instrument ref; `hypothesized` otherwise (a bare
/// `rule_outcome` or source/inspection ref does not evidence).
pub fn evidence_grade(refs: &[EvidenceRef]) -> EvidenceGrade {
    let instruments: BTreeSet<EvidenceKind> = refs
        .iter()
        .map(|r| r.kind)
        .filter(|k| k.is_instrument())
        .collect();
    if instruments.len() >= 2 {
        EvidenceGrade::Confirmed
    } else if !instruments.is_empty() {
        EvidenceGrade::Evidenced
    } else {
        EvidenceGrade::Hypothesized
    }
}

/// `EffectDirection` — the closed direction sum for `PredictedEffect`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EffectDirection {
    /// `increase` — the deficit increases the metric.
    Increase,
    /// `decrease` — the deficit decreases it.
    Decrease,
    /// `ambiguous` — direction is not asserted.
    Ambiguous,
}

impl EffectDirection {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            EffectDirection::Increase => "increase",
            EffectDirection::Decrease => "decrease",
            EffectDirection::Ambiguous => "ambiguous",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<EffectDirection> {
        match s {
            "increase" => Some(EffectDirection::Increase),
            "decrease" => Some(EffectDirection::Decrease),
            "ambiguous" => Some(EffectDirection::Ambiguous),
            _ => None,
        }
    }
}

/// `PredictedEffect` — the debt's predicted effect on the measured outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredictedEffect {
    /// `increase{metric?, magnitude?}` — predicted to inflate the metric.
    Increase,
    /// `decrease{metric?, magnitude?}` — predicted to deflate it.
    Decrease,
    /// `ambiguous` — direction not asserted.
    Ambiguous,
    /// `zero` — the debt is predicted to have no effect (dead weight).
    Zero,
    /// `qualitative` — a qualitative (non-quantified) prediction.
    Qualitative,
}

impl PredictedEffect {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            PredictedEffect::Increase => "increase",
            PredictedEffect::Decrease => "decrease",
            PredictedEffect::Ambiguous => "ambiguous",
            PredictedEffect::Zero => "zero",
            PredictedEffect::Qualitative => "qualitative",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<PredictedEffect> {
        match s {
            "increase" => Some(PredictedEffect::Increase),
            "decrease" => Some(PredictedEffect::Decrease),
            "ambiguous" => Some(PredictedEffect::Ambiguous),
            "zero" => Some(PredictedEffect::Zero),
            "qualitative" => Some(PredictedEffect::Qualitative),
            _ => None,
        }
    }
}

/// `ModelSelector` — the model-scope selector sum (§5h.6 §3): `exact{model_id}`
/// or `range{family, version_predicate}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelSelector {
    /// `exact{model_id}` — a single model.
    Exact {
        /// The model id.
        model_id: String,
    },
    /// `range{family, version_predicate}` — a family/version range.
    Range {
        /// The model family.
        family: String,
        /// The version predicate.
        version_predicate: String,
    },
}

/// `DebtScope{model_selectors[], task_classes[], roles[]}` — the debt's
/// applicability scope (§5h.6 §3). `model_selectors` is the member a
/// model-conditioned/`evolution-origin` debt must carry
/// (`missing_snapshot_scope` when absent).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DebtScope {
    /// The model selectors the debt's claim is conditioned on.
    pub model_selectors: Vec<ModelSelector>,
    /// The task classes the debt applies to.
    pub task_classes: Vec<String>,
    /// The roles the debt applies to.
    pub roles: Vec<String>,
}

/// `HypothesisSubject` — what `hypothesis_typed.subject` names (§5h.6 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HypothesisSubject {
    /// `deficiency` — the hypothesis asserts a deficiency exists.
    Deficiency,
    /// `relevance` — the hypothesis asserts a deficiency is relevant to the
    /// outcome.
    Relevance,
    /// `association` — the hypothesis asserts an association.
    Association,
    /// `removal_benefit` — the hypothesis asserts removing the deficiency
    /// benefits the outcome.
    RemovalBenefit,
}

impl HypothesisSubject {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            HypothesisSubject::Deficiency => "deficiency",
            HypothesisSubject::Relevance => "relevance",
            HypothesisSubject::Association => "association",
            HypothesisSubject::RemovalBenefit => "removal_benefit",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<HypothesisSubject> {
        match s {
            "deficiency" => Some(HypothesisSubject::Deficiency),
            "relevance" => Some(HypothesisSubject::Relevance),
            "association" => Some(HypothesisSubject::Association),
            "removal_benefit" => Some(HypothesisSubject::RemovalBenefit),
            _ => None,
        }
    }
}

/// `HypothesisTyped{subject, deficiency_class, predicted_effect}` — the typed
/// hypothesis the `/1` record carries alongside the `hypothesis: Text` leaf
/// (§5h.6 §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HypothesisTyped {
    /// What the hypothesis asserts.
    pub subject: HypothesisSubject,
    /// The closed deficiency class (`unknown` admissible only with
    /// `evidence_grade = hypothesized`).
    pub deficiency_class: DeficiencyClass,
    /// The predicted effect of the deficiency.
    pub predicted_effect: PredictedEffect,
}

/// `RemovalTestKind` — the closed removal-test sum (§5h.6 §3): how the debt's
/// claim is falsified/discharged. Defaulted per home (the `DebtHomes/1`
/// `removal_test.kind` column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RemovalTestKind {
    /// `inspection` — human inspection.
    Inspection,
    /// `probe_run` — a probe run.
    ProbeRun,
    /// `model_probe` — a model-behavior probe.
    ModelProbe,
    /// `zero_uses` — the debt discharges when the artifact has zero uses.
    ZeroUses,
    /// `conformance_run` — a conformance run.
    ConformanceRun,
    /// `schema_version_bound` — discharges at a schema-version bound.
    SchemaVersionBound,
    /// `challenge_run` — a challenge/red-team run.
    ChallengeRun,
    /// `retirement_experiment` — a retirement-diff experiment (the canonical
    /// discharge path for conditioned claims).
    RetirementExperiment,
    /// `attestation` — an attestation discharges it.
    Attestation,
    /// `schema` — a schema-level check.
    Schema,
    /// `documentation` — a documentation check.
    Documentation,
    /// `evidence_superseded` — the evidence refs were superseded/refreshed.
    EvidenceSuperseded,
}

impl RemovalTestKind {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            RemovalTestKind::Inspection => "inspection",
            RemovalTestKind::ProbeRun => "probe_run",
            RemovalTestKind::ModelProbe => "model_probe",
            RemovalTestKind::ZeroUses => "zero_uses",
            RemovalTestKind::ConformanceRun => "conformance_run",
            RemovalTestKind::SchemaVersionBound => "schema_version_bound",
            RemovalTestKind::ChallengeRun => "challenge_run",
            RemovalTestKind::RetirementExperiment => "retirement_experiment",
            RemovalTestKind::Attestation => "attestation",
            RemovalTestKind::Schema => "schema",
            RemovalTestKind::Documentation => "documentation",
            RemovalTestKind::EvidenceSuperseded => "evidence_superseded",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<RemovalTestKind> {
        match s {
            "inspection" => Some(RemovalTestKind::Inspection),
            "probe_run" => Some(RemovalTestKind::ProbeRun),
            "model_probe" => Some(RemovalTestKind::ModelProbe),
            "zero_uses" => Some(RemovalTestKind::ZeroUses),
            "conformance_run" => Some(RemovalTestKind::ConformanceRun),
            "schema_version_bound" => Some(RemovalTestKind::SchemaVersionBound),
            "challenge_run" => Some(RemovalTestKind::ChallengeRun),
            "retirement_experiment" => Some(RemovalTestKind::RetirementExperiment),
            "attestation" => Some(RemovalTestKind::Attestation),
            "schema" => Some(RemovalTestKind::Schema),
            "documentation" => Some(RemovalTestKind::Documentation),
            "evidence_superseded" => Some(RemovalTestKind::EvidenceSuperseded),
            _ => None,
        }
    }
}

/// `RemovalTest{kind, …}` — the typed removal test the `/1` record carries
/// (§5h.6 §3). Kind-specific members ride the variant payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovalTest {
    /// The closed kind sum.
    pub kind: RemovalTestKind,
    /// `retirement_experiment`: the experiment-template ref the test
    /// instantiates (`template_ref` — resolves at validation).
    pub template_ref: Option<String>,
    /// `retirement_experiment`: the claimed beneficiaries — a set of
    /// `(factor, level)` pairs the retirement diff claims to benefit; each
    /// must be `⊆ scope` (`beneficiaries ⊆ scope` refusal).
    pub beneficiaries: Vec<String>,
    /// `probe_run`/`model_probe`/`challenge_run`: the probe spec refs the test
    /// resolves.
    pub probe_refs: Vec<String>,
    /// `zero_uses`: the artifact scope the zero-use check applies to.
    pub scope_ref: Option<String>,
    /// `schema_version_bound`: the version bound.
    pub version_bound: Option<String>,
    /// `conformance_run`: the conformance suite ref.
    pub conformance_suite_ref: Option<String>,
    /// `attestation`: the attestation ref.
    pub attestation_ref: Option<String>,
    /// `inspection`/`schema`/`documentation`: the checklist/criteria text.
    pub criteria: Option<String>,
    /// `evidence_superseded`: the evidence refs the superseding evidence must
    /// cover.
    pub supersedes_refs: Vec<String>,
    /// The deadline the test must be scheduled by (transaction time), if bound.
    pub deadline: Option<u64>,
}

impl RemovalTest {
    /// A bare `kind` removal test (no payload members).
    pub fn new(kind: RemovalTestKind) -> RemovalTest {
        RemovalTest {
            kind,
            template_ref: None,
            beneficiaries: Vec::new(),
            probe_refs: Vec::new(),
            scope_ref: None,
            version_bound: None,
            conformance_suite_ref: None,
            attestation_ref: None,
            criteria: None,
            supersedes_refs: Vec::new(),
            deadline: None,
        }
    }

    /// Whether the test *instantiates* — has the payload its kind requires
    /// (`retirement_experiment` needs a `template_ref`; `zero_uses` a
    /// `scope_ref`; `schema_version_bound` a `version_bound`; etc.). The full
    /// instantiation check lives in `hh-hir::debt::validate_removal_test`;
    /// this is the member-level half.
    pub fn instantiates(&self) -> bool {
        match self.kind {
            RemovalTestKind::RetirementExperiment => self.template_ref.is_some(),
            RemovalTestKind::ZeroUses => self.scope_ref.is_some(),
            RemovalTestKind::SchemaVersionBound => self.version_bound.is_some(),
            RemovalTestKind::ConformanceRun => self.conformance_suite_ref.is_some(),
            RemovalTestKind::Attestation => self.attestation_ref.is_some(),
            RemovalTestKind::ProbeRun
            | RemovalTestKind::ModelProbe
            | RemovalTestKind::ChallengeRun => !self.probe_refs.is_empty(),
            RemovalTestKind::Inspection
            | RemovalTestKind::Schema
            | RemovalTestKind::Documentation => self.criteria.is_some(),
            RemovalTestKind::EvidenceSuperseded => !self.supersedes_refs.is_empty(),
        }
    }
}

/// `Verdict` — the closed removal-test verdict sum (§5h.6 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Verdict {
    /// `pass` — the test discharged the debt.
    Pass,
    /// `fail` — the test falsified the discharge claim.
    Fail,
    /// `inconclusive` — the test could not settle.
    Inconclusive,
}

impl Verdict {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            Verdict::Pass => "pass",
            Verdict::Fail => "fail",
            Verdict::Inconclusive => "inconclusive",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<Verdict> {
        match s {
            "pass" => Some(Verdict::Pass),
            "fail" => Some(Verdict::Fail),
            "inconclusive" => Some(Verdict::Inconclusive),
            _ => None,
        }
    }
}

/// `RemovalVerdict{debt_ref, kind, verdict, report_ref, settled_at}` — the
/// settled outcome of a removal test (§5h.6 §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovalVerdict {
    /// The debt record the verdict settles.
    pub debt_ref: String,
    /// The removal-test kind that ran.
    pub kind: RemovalTestKind,
    /// The verdict (`inconclusive` carries `reason` — decoded from the member).
    pub verdict: Verdict,
    /// The `inconclusive` reason, when the verdict is inconclusive.
    pub reason: Option<String>,
    /// The report the verdict cites.
    pub report_ref: String,
    /// When the test settled (transaction time).
    pub settled_at: u64,
}

/// `OwnerRef` — the debt owner (§5h.6 §3): `principal{id}` or `team{id}`, with
/// an optional `reach_via` sink set (OQ-446 interim: `insufficient_runway` and
/// `owner_unreachable` classify together until the sinks land).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerRef {
    /// Whether the owner is a principal or a team.
    pub team: bool,
    /// The principal/team id.
    pub id: String,
    /// The notification sinks the owner is reachable through.
    pub reach_via: Vec<String>,
}

impl OwnerRef {
    /// A `principal{id}` owner.
    pub fn principal(id: impl Into<String>) -> OwnerRef {
        OwnerRef {
            team: false,
            id: id.into(),
            reach_via: Vec::new(),
        }
    }

    /// A `team{id}` owner.
    pub fn team(id: impl Into<String>) -> OwnerRef {
        OwnerRef {
            team: true,
            id: id.into(),
            reach_via: Vec::new(),
        }
    }

    /// Whether the owner is reachable through at least one declared sink —
    /// the `OwnerUnreachable` check's member-level half (the context supplies
    /// the declared-sink set).
    pub fn has_reach_sinks(&self) -> bool {
        !self.reach_via.is_empty()
    }
}

/// `DebtRef{kind, ref}` — a typed reference to a debt record (§5h.6 §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebtRef {
    /// The debt record kind.
    pub kind: String,
    /// The record ref.
    pub reference: String,
}

/// `RevalidationOn` — the closed revalidation-trigger sum (§5h.6 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RevalidationOn {
    /// `evidence_stale` — the evidence refs aged past `evidence_max_age`.
    EvidenceStale,
    /// `model_change` — a model in the scope changed.
    ModelChange,
    /// `drift_detected` — a drift bracket fired.
    DriftDetected,
    /// `schedule` — the periodic schedule.
    Schedule,
}

impl RevalidationOn {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            RevalidationOn::EvidenceStale => "evidence_stale",
            RevalidationOn::ModelChange => "model_change",
            RevalidationOn::DriftDetected => "drift_detected",
            RevalidationOn::Schedule => "schedule",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<RevalidationOn> {
        match s {
            "evidence_stale" => Some(RevalidationOn::EvidenceStale),
            "model_change" => Some(RevalidationOn::ModelChange),
            "drift_detected" => Some(RevalidationOn::DriftDetected),
            "schedule" => Some(RevalidationOn::Schedule),
            _ => None,
        }
    }
}

/// `RevalidationAction` — the closed revalidation-action sum (§5h.6 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RevalidationAction {
    /// `re_probe` — re-run the probe.
    ReProbe,
    /// `re_experiment` — re-run the experiment.
    ReExperiment,
    /// `refresh_evidence` — refresh the evidence refs.
    RefreshEvidence,
    /// `human_review` — escalate to human review.
    HumanReview,
}

impl RevalidationAction {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            RevalidationAction::ReProbe => "re_probe",
            RevalidationAction::ReExperiment => "re_experiment",
            RevalidationAction::RefreshEvidence => "refresh_evidence",
            RevalidationAction::HumanReview => "human_review",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<RevalidationAction> {
        match s {
            "re_probe" => Some(RevalidationAction::ReProbe),
            "re_experiment" => Some(RevalidationAction::ReExperiment),
            "refresh_evidence" => Some(RevalidationAction::RefreshEvidence),
            "human_review" => Some(RevalidationAction::HumanReview),
            _ => None,
        }
    }
}

/// `Revalidation{on[], action}` — the debt's revalidation policy (§5h.6 §3).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Revalidation {
    /// The triggers.
    pub on: Vec<RevalidationOn>,
    /// The action taken when a trigger fires.
    pub action: Option<RevalidationAction>,
}

/// `DebtHome` — one row of `DebtHomes/1`: the closed inventory of record
/// kinds permitted to carry `AssumptionDebtRecord/1` (§5h.6 §2; ADR-0197).
/// A debt record on an unlisted kind is `UnknownDebtHome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebtHome {
    /// The home's stable id (row number in the inventory).
    pub id: u8,
    /// The record kind the home names (the canonical kind spelling).
    pub record_kind: &'static str,
    /// The field on the record that carries the debt.
    pub field: &'static str,
    /// The default `debt_class` for the home.
    pub debt_class: DebtClass,
    /// The default `expiry_condition.kind`.
    pub expiry_kind: ExpiryKind,
    /// The default `RemovalTest.kind`.
    pub removal_test_kind: RemovalTestKind,
    /// Whether `scope.model_selectors` is mandatory here (model-conditioned
    /// homes — `missing_snapshot_scope` when absent).
    pub needs_model_scope: bool,
}

/// `DebtHomes/1` — the closed 17-row inventory (§5h.6 §2 table). Every
/// `AssumptionDebtRecord` is written on a listed home at a listed field; a
/// record on an unlisted kind is `UnknownDebtHome`.
pub const DEBT_HOMES: &[DebtHome] = &[
    DebtHome {
        id: 1,
        record_kind: "harness_rule",
        field: "assumption_debt",
        debt_class: DebtClass::Hypothesized,
        expiry_kind: ExpiryKind::EvidenceRefreshDue,
        removal_test_kind: RemovalTestKind::RetirementExperiment,
        needs_model_scope: true,
    },
    DebtHome {
        id: 2,
        record_kind: "profile_rule",
        field: "debt",
        debt_class: DebtClass::ModelConditioned,
        expiry_kind: ExpiryKind::ModelVersionChange,
        removal_test_kind: RemovalTestKind::RetirementExperiment,
        needs_model_scope: true,
    },
    DebtHome {
        id: 3,
        record_kind: "model_profile",
        field: "expiry",
        debt_class: DebtClass::ModelConditioned,
        expiry_kind: ExpiryKind::ModelVersionChange,
        removal_test_kind: RemovalTestKind::RetirementExperiment,
        needs_model_scope: true,
    },
    DebtHome {
        id: 4,
        record_kind: "model_profile_ext",
        field: "debt",
        debt_class: DebtClass::ModelConditioned,
        expiry_kind: ExpiryKind::ModelVersionChange,
        removal_test_kind: RemovalTestKind::RetirementExperiment,
        needs_model_scope: true,
    },
    DebtHome {
        id: 5,
        record_kind: "fallback_profile",
        field: "debt",
        debt_class: DebtClass::ModelConditioned,
        expiry_kind: ExpiryKind::ModelVersionChange,
        removal_test_kind: RemovalTestKind::RetirementExperiment,
        needs_model_scope: true,
    },
    DebtHome {
        id: 6,
        record_kind: "wire_dialect_rule",
        field: "debt",
        debt_class: DebtClass::Empirical,
        expiry_kind: ExpiryKind::EvidenceRefreshDue,
        removal_test_kind: RemovalTestKind::Schema,
        needs_model_scope: false,
    },
    DebtHome {
        id: 7,
        record_kind: "routing_policy_conditioned_rule",
        field: "debt",
        debt_class: DebtClass::ModelConditioned,
        expiry_kind: ExpiryKind::ModelVersionChange,
        removal_test_kind: RemovalTestKind::RetirementExperiment,
        needs_model_scope: true,
    },
    DebtHome {
        id: 8,
        record_kind: "harness_rule_critic_gate",
        field: "assumption_debt",
        debt_class: DebtClass::Hypothesized,
        expiry_kind: ExpiryKind::EvidenceRefreshDue,
        removal_test_kind: RemovalTestKind::RetirementExperiment,
        needs_model_scope: false,
    },
    DebtHome {
        id: 9,
        record_kind: "calibration_record",
        field: "expiry",
        debt_class: DebtClass::Empirical,
        expiry_kind: ExpiryKind::EvidenceRefreshDue,
        removal_test_kind: RemovalTestKind::ProbeRun,
        needs_model_scope: false,
    },
    DebtHome {
        id: 10,
        record_kind: "surface_variant",
        field: "debt",
        debt_class: DebtClass::Hypothesized,
        expiry_kind: ExpiryKind::EvidenceRefreshDue,
        removal_test_kind: RemovalTestKind::RetirementExperiment,
        needs_model_scope: true,
    },
    DebtHome {
        id: 11,
        record_kind: "fitted_surface_report",
        field: "debt_record_ref",
        debt_class: DebtClass::Empirical,
        expiry_kind: ExpiryKind::EvidenceRefreshDue,
        removal_test_kind: RemovalTestKind::RetirementExperiment,
        needs_model_scope: false,
    },
    DebtHome {
        id: 12,
        record_kind: "adapter_record",
        field: "debt",
        debt_class: DebtClass::Empirical,
        expiry_kind: ExpiryKind::EvidenceRefreshDue,
        removal_test_kind: RemovalTestKind::ConformanceRun,
        needs_model_scope: false,
    },
    DebtHome {
        id: 13,
        record_kind: "budget_enforcement_derivation",
        field: "debt",
        debt_class: DebtClass::Empirical,
        expiry_kind: ExpiryKind::EvidenceRefreshDue,
        removal_test_kind: RemovalTestKind::Schema,
        needs_model_scope: false,
    },
    DebtHome {
        id: 14,
        record_kind: "protocol_era_item",
        field: "debt",
        debt_class: DebtClass::Unclassified,
        expiry_kind: ExpiryKind::EvidenceRefreshDue,
        removal_test_kind: RemovalTestKind::Documentation,
        needs_model_scope: false,
    },
    DebtHome {
        id: 15,
        record_kind: "hot_path_ceiling",
        field: "debt",
        debt_class: DebtClass::Empirical,
        expiry_kind: ExpiryKind::EvidenceRefreshDue,
        removal_test_kind: RemovalTestKind::ProbeRun,
        needs_model_scope: false,
    },
    DebtHome {
        id: 16,
        record_kind: "assumption_debt_manager",
        field: "debt",
        debt_class: DebtClass::Unclassified,
        expiry_kind: ExpiryKind::EvidenceRefreshDue,
        removal_test_kind: RemovalTestKind::Inspection,
        needs_model_scope: false,
    },
    DebtHome {
        id: 17,
        record_kind: "adr",
        field: "debt",
        debt_class: DebtClass::Unclassified,
        expiry_kind: ExpiryKind::EvidenceRefreshDue,
        removal_test_kind: RemovalTestKind::Documentation,
        needs_model_scope: false,
    },
];

/// The `DebtHomes/1` lookup — the home row for a `(record_kind, field)` pair,
/// or `None` (→ `UnknownDebtHome`).
pub fn debt_home(record_kind: &str, field: &str) -> Option<&'static DebtHome> {
    DEBT_HOMES
        .iter()
        .find(|h| h.record_kind == record_kind && h.field == field)
}

/// `UnexecutableReason` — why a removal test cannot instantiate (§5h.6 §4;
/// the `UnexecutableRemovalTest` refusal payload).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnexecutableReason {
    /// `unresolved_template` — a `retirement_experiment`'s `template_ref` did
    /// not resolve to an experiment template.
    UnresolvedTemplate,
    /// `missing_match_spec` — the resolved template carries no `match_spec`
    /// (a retirement experiment is a matched-budget design).
    MissingMatchSpec,
    /// `not_a_retirement_diff` — the diff the test would run is not a
    /// single-rule removal.
    NotARetirementDiff,
    /// `unresolved_probe_ref` — a `probe_run`/`model_probe`/`challenge_run`
    /// ref did not resolve.
    UnresolvedProbeRef,
    /// `unresolved_zero_use_scope` — a `zero_uses` test's scope did not
    /// resolve.
    UnresolvedZeroUseScope,
    /// `missing_split_assignment` — the test needs a split assignment that
    /// is absent.
    MissingSplitAssignment,
    /// `unsealed_artifact` — a required artifact is unsealed.
    UnsealedArtifact,
    /// `missing_snapshot_scope` — a model-conditioned/evolution-origin record
    /// lacks `scope.model_selectors`.
    MissingSnapshotScope,
    /// `beneficiaries_outside_scope` — a claimed `(factor, level)` beneficiary
    /// is not `⊆ scope`.
    BeneficiariesOutsideScope,
    /// `missing_payload` — the kind's mandatory payload member is absent
    /// (`retirement_experiment` without `template_ref`, etc.).
    MissingPayload,
}

impl UnexecutableReason {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            UnexecutableReason::UnresolvedTemplate => "unresolved_template",
            UnexecutableReason::MissingMatchSpec => "missing_match_spec",
            UnexecutableReason::NotARetirementDiff => "not_a_retirement_diff",
            UnexecutableReason::UnresolvedProbeRef => "unresolved_probe_ref",
            UnexecutableReason::UnresolvedZeroUseScope => "unresolved_zero_use_scope",
            UnexecutableReason::MissingSplitAssignment => "missing_split_assignment",
            UnexecutableReason::UnsealedArtifact => "unsealed_artifact",
            UnexecutableReason::MissingSnapshotScope => "missing_snapshot_scope",
            UnexecutableReason::BeneficiariesOutsideScope => "beneficiaries_outside_scope",
            UnexecutableReason::MissingPayload => "missing_payload",
        }
    }
}

/// `DebtPolicy` — the assumption-debt policy record (§5h.6 §5): the tunable
/// constants and per-home required-field table the manager enforces. Defaults
/// are the proposed placeholder values the ADR-0214 deferral text records —
/// the ratified values land with the manager (Stage 5).
#[derive(Debug, Clone, PartialEq)]
pub struct DebtPolicy {
    /// Per-home required-field overrides: `record_kind.field → field names`
    /// beyond the ratified base `{rule_id, hypothesis, evidence_refs, owner,
    /// expiry_condition, removal_test_ref, status}`.
    pub required_fields_by_home: BTreeMap<String, Vec<String>>,
    /// `min_runway` — the minimum ms a `date`-bounded expiry must leave
    /// (`InsufficientRunway` below it).
    pub min_runway_ms: u64,
    /// `hypothesized_max_age` — the max age a hypothesized-evidence debt may
    /// sit before revalidation.
    pub hypothesized_max_age_ms: u64,
    /// `evidence_max_age` — the max evidence-ref age.
    pub evidence_max_age_ms: u64,
    /// `warn_within` — the window before expiry a debt moves to `expiring`.
    pub warn_within_ms: u64,
    /// `grace_period` — the post-expiry grace before escalation.
    pub grace_period_ms: u64,
    /// `max_open_removal_tests` — the removal-test schedule bound.
    pub max_open_removal_tests: u32,
    /// `priority` — the scheduling priority order spelling.
    pub priority: String,
    /// `notice_sinks` — the declared notification sink ids.
    pub notice_sinks: Vec<String>,
    /// `schedule` — the manager's check schedule (a cron-ish spelling).
    pub schedule: String,
    /// `dead_weight_designs_allowed` — whether zero-effect (dead-weight)
    /// removal designs are admissible.
    pub dead_weight_designs_allowed: bool,
}

impl Default for DebtPolicy {
    /// The proposed placeholder defaults (§5h.6 §5; the ADR-0214 deferral
    /// records these as proposed — the ratified values land with the
    /// manager's Stage-5 tuning).
    fn default() -> DebtPolicy {
        DebtPolicy {
            required_fields_by_home: BTreeMap::new(),
            min_runway_ms: 30 * 24 * 3600 * 1000, // 30 days
            hypothesized_max_age_ms: 90 * 24 * 3600 * 1000, // 90 days
            evidence_max_age_ms: 180 * 24 * 3600 * 1000, // 180 days
            warn_within_ms: 14 * 24 * 3600 * 1000, // 14 days
            grace_period_ms: 30 * 24 * 3600 * 1000, // 30 days
            max_open_removal_tests: 64,
            priority: "expiry_urgency".into(),
            notice_sinks: Vec::new(),
            schedule: "daily".into(),
            dead_weight_designs_allowed: true,
        }
    }
}

/// The ratified required-field base every home carries (`AssumptionDebtRecord/1`
/// §5h.6 §3): `{rule_id, hypothesis, evidence_refs, owner, expiry_condition,
/// removal_test_ref, status}` — the `/1` additive members are per-home policy.
pub const REQUIRED_FIELDS_BASE: &[&str] = &[
    "rule_id",
    "hypothesis",
    "evidence_refs",
    "owner",
    "expiry_condition",
    "removal_test_ref",
    "status",
];

/// `required_fields(home, policy)` — the fields a debt record at `home` must
/// carry: the ratified base ∪ the home's policy override ∪ the home's
/// defaults (`debt_class`, `removal_test` where the home's defaults declare
/// them, `scope.model_selectors` where `needs_model_scope`).
pub fn required_fields(home: &DebtHome, policy: &DebtPolicy) -> BTreeSet<String> {
    let mut set: BTreeSet<String> = REQUIRED_FIELDS_BASE.iter().map(|s| (*s).into()).collect();
    set.insert("debt_class".into());
    set.insert("removal_test".into());
    if home.needs_model_scope {
        set.insert("scope.model_selectors".into());
    }
    let key = format!("{}.{}", home.record_kind, home.field);
    if let Some(extra) = policy.required_fields_by_home.get(&key) {
        for f in extra {
            set.insert(f.clone());
        }
    }
    set
}

/// `AssumptionDebtHealth` — the closed debt-health metric-name set (§5h.6 §5):
/// the `debt.*` metrics the health panel declares.
pub const DEBT_HEALTH_METRICS: &[&str] = &[
    "debt.open_by_class",
    "debt.expiring_within_30d",
    "debt.expired_unretired",
    "debt.removal_tests.open",
    "debt.removal_tests.settled_pass",
    "debt.removal_tests.settled_fail",
    "debt.removal_tests.settled_inconclusive",
    "debt.evidence_grade.hypothesized",
    "debt.evidence_grade.evidenced",
    "debt.evidence_grade.confirmed",
];

// ─────────────────────────────────────────────────────────────────────────────
// Canonical JSON codecs (CC7 — one schema source; hh-hir/hh-compiler/hh-registry
// delegate to these for the plain-data members).
// ─────────────────────────────────────────────────────────────────────────────

/// The codec refusal — a missing/unknown member on decode. Carried as a plain
/// detail string; callers map it onto their own violation type
/// (`HirError::SchemaViolation`, registry `SchemaViolation`, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebtSchemaError {
    /// The violation detail (`path.member: what's wrong`).
    pub detail: String,
}

fn bad(detail: impl Into<String>) -> DebtSchemaError {
    DebtSchemaError {
        detail: detail.into(),
    }
}

fn req<'a>(j: &'a Json, name: &str, path: &str) -> Result<&'a Json, DebtSchemaError> {
    match j {
        Json::Obj(m) => m
            .get(name)
            .ok_or_else(|| bad(format!("{path}.{name} missing"))),
        _ => Err(bad(format!("{path} is not an object"))),
    }
}

fn opt<'a>(j: &'a Json, name: &str) -> Option<&'a Json> {
    match j {
        Json::Obj(m) => m.get(name),
        _ => None,
    }
}

fn req_str(j: &Json, name: &str, path: &str) -> Result<String, DebtSchemaError> {
    req(j, name, path)?
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| bad(format!("{path}.{name} is not a string")))
}

fn opt_str(j: &Json, name: &str, path: &str) -> Result<Option<String>, DebtSchemaError> {
    match opt(j, name) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => v
            .as_str()
            .map(|s| Some(s.to_string()))
            .ok_or_else(|| bad(format!("{path}.{name} is not a string"))),
    }
}

fn opt_u64(j: &Json, name: &str, path: &str) -> Result<Option<u64>, DebtSchemaError> {
    match opt(j, name) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => v
            .as_int()
            .map(|i| Some(i.max(0) as u64))
            .ok_or_else(|| bad(format!("{path}.{name} is not an integer"))),
    }
}

fn opt_bool(j: &Json, name: &str) -> bool {
    matches!(opt(j, name), Some(Json::Bool(true)))
}

fn str_arr(j: &Json, name: &str, path: &str) -> Result<Vec<String>, DebtSchemaError> {
    match opt(j, name) {
        None | Some(Json::Null) => Ok(Vec::new()),
        Some(Json::Arr(items)) => items
            .iter()
            .enumerate()
            .map(|(i, v)| {
                v.as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| bad(format!("{path}.{name}[{i}] is not a string")))
            })
            .collect(),
        _ => Err(bad(format!("{path}.{name} is not an array"))),
    }
}

impl ExpiryCondition {
    /// Decode `{kind, value?}` — or a bare spelling `"{kind}"` (legacy bodies).
    pub fn from_json(j: &Json, path: &str) -> Result<ExpiryCondition, DebtSchemaError> {
        match j {
            Json::Str(s) => {
                let kind = ExpiryKind::parse(s)
                    .ok_or_else(|| bad(format!("{path}: unknown expiry kind {s}")))?;
                Ok(ExpiryCondition { kind, value: None })
            }
            Json::Obj(_) => {
                let ks = req_str(j, "kind", path)?;
                let kind = ExpiryKind::parse(&ks)
                    .ok_or_else(|| bad(format!("{path}.kind: unknown expiry kind {ks}")))?;
                Ok(ExpiryCondition {
                    kind,
                    value: opt_str(j, "value", path)?,
                })
            }
            _ => Err(bad(format!("{path} is not an expiry_condition"))),
        }
    }
}

impl EvidenceRef {
    /// Decode `{kind, ref, observed_at?, tier?, provisional?}` — or a bare
    /// string (legacy `evidence_refs` members → `{kind: source, ref}`).
    pub fn from_json(j: &Json, path: &str) -> Result<EvidenceRef, DebtSchemaError> {
        match j {
            Json::Str(s) => Ok(EvidenceRef::legacy(s.clone())),
            Json::Obj(_) => {
                let ks = req_str(j, "kind", path)?;
                let kind = EvidenceKind::parse(&ks)
                    .ok_or_else(|| bad(format!("{path}.kind: unknown evidence kind {ks}")))?;
                Ok(EvidenceRef {
                    kind,
                    reference: req_str(j, "ref", path)?,
                    observed_at: opt_u64(j, "observed_at", path)?,
                    tier: opt_str(j, "tier", path)?,
                    provisional: opt_bool(j, "provisional"),
                })
            }
            _ => Err(bad(format!("{path} is not an evidence_ref"))),
        }
    }
}

impl DeficiencyClass {
    /// Decode a bare spelling or `{unknown: "…"}`.
    pub fn from_json(j: &Json, path: &str) -> Result<DeficiencyClass, DebtSchemaError> {
        match j {
            Json::Str(s) => DeficiencyClass::parse(s)
                .ok_or_else(|| bad(format!("{path}: unknown deficiency_class {s}"))),
            Json::Obj(m) => {
                let text = m.get("unknown").and_then(|v| v.as_str()).ok_or_else(|| {
                    bad(format!(
                        "{path}: deficiency_class object must be {{unknown: text}}"
                    ))
                })?;
                Ok(DeficiencyClass::Unknown(text.to_string()))
            }
            _ => Err(bad(format!("{path} is not a deficiency_class"))),
        }
    }
}

impl PredictedEffect {
    /// The canonical JSON (a bare spelling).
    pub fn to_json(&self) -> Json {
        Json::str(self.name())
    }
}

impl ModelSelector {
    /// The canonical JSON — `{exact: model_id}` | `{range: {family, version_predicate}}`.
    pub fn to_json(&self) -> Json {
        match self {
            ModelSelector::Exact { model_id } => {
                Json::obj([("exact", Json::str(model_id.clone()))])
            }
            ModelSelector::Range {
                family,
                version_predicate,
            } => Json::obj([(
                "range",
                Json::obj([
                    ("family", Json::str(family.clone())),
                    ("version_predicate", Json::str(version_predicate.clone())),
                ]),
            )]),
        }
    }

    /// Decode `{exact: model_id}` | `{range: {family, version_predicate}}`.
    pub fn from_json(j: &Json, path: &str) -> Result<ModelSelector, DebtSchemaError> {
        if let Some(v) = opt(j, "exact") {
            let model_id = v
                .as_str()
                .ok_or_else(|| bad(format!("{path}.exact is not a string")))?;
            return Ok(ModelSelector::Exact {
                model_id: model_id.to_string(),
            });
        }
        if let Some(v) = opt(j, "range") {
            return Ok(ModelSelector::Range {
                family: req_str(v, "family", &format!("{path}.range"))?,
                version_predicate: req_str(v, "version_predicate", &format!("{path}.range"))?,
            });
        }
        Err(bad(format!("{path} is not a model_selector")))
    }
}

impl DebtScope {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        if !self.model_selectors.is_empty() {
            m.insert(
                "model_selectors".into(),
                Json::Arr(
                    self.model_selectors
                        .iter()
                        .map(ModelSelector::to_json)
                        .collect(),
                ),
            );
        }
        if !self.task_classes.is_empty() {
            m.insert(
                "task_classes".into(),
                Json::Arr(
                    self.task_classes
                        .iter()
                        .map(|s| Json::str(s.clone()))
                        .collect(),
                ),
            );
        }
        if !self.roles.is_empty() {
            m.insert(
                "roles".into(),
                Json::Arr(self.roles.iter().map(|s| Json::str(s.clone())).collect()),
            );
        }
        Json::Obj(m)
    }

    /// Decode `{model_selectors[]?, task_classes[]?, roles[]?}`.
    pub fn from_json(j: &Json, path: &str) -> Result<DebtScope, DebtSchemaError> {
        let mut model_selectors = Vec::new();
        if let Some(Json::Arr(items)) = opt(j, "model_selectors") {
            for (i, v) in items.iter().enumerate() {
                model_selectors.push(ModelSelector::from_json(
                    v,
                    &format!("{path}.model_selectors[{i}]"),
                )?);
            }
        }
        Ok(DebtScope {
            model_selectors,
            task_classes: str_arr(j, "task_classes", path)?,
            roles: str_arr(j, "roles", path)?,
        })
    }

    /// The flattened scope spellings for the `beneficiaries ⊆ scope` check:
    /// `model_snapshot:<id>` per `Exact` selector (a `Range` covers any
    /// `model_snapshot:` beneficiary inside the family prefix), `task:<class>`
    /// per task class, `role:<role>` per role.
    pub fn covers_beneficiary(&self, beneficiary: &str) -> bool {
        if let Some(id) = beneficiary.strip_prefix("model_snapshot:") {
            return self.model_selectors.iter().any(|s| match s {
                ModelSelector::Exact { model_id } => model_id == id,
                ModelSelector::Range { family, .. } => id.starts_with(family.as_str()),
            });
        }
        if let Some(class) = beneficiary.strip_prefix("task:") {
            return self.task_classes.iter().any(|c| c == class);
        }
        if let Some(role) = beneficiary.strip_prefix("role:") {
            return self.roles.iter().any(|r| r == role);
        }
        false
    }
}

impl HypothesisTyped {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("subject", Json::str(self.subject.name())),
            ("deficiency_class", self.deficiency_class.to_json()),
            ("predicted_effect", self.predicted_effect.to_json()),
        ])
    }

    /// Decode `{subject, deficiency_class, predicted_effect}`.
    pub fn from_json(j: &Json, path: &str) -> Result<HypothesisTyped, DebtSchemaError> {
        let ss = req_str(j, "subject", path)?;
        Ok(HypothesisTyped {
            subject: HypothesisSubject::parse(&ss)
                .ok_or_else(|| bad(format!("{path}.subject: unknown subject {ss}")))?,
            deficiency_class: DeficiencyClass::from_json(
                req(j, "deficiency_class", path)?,
                &format!("{path}.deficiency_class"),
            )?,
            predicted_effect: {
                let ps = req_str(j, "predicted_effect", path)?;
                PredictedEffect::parse(&ps)
                    .ok_or_else(|| bad(format!("{path}.predicted_effect: unknown effect {ps}")))?
            },
        })
    }
}

impl ExpiryParams {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        if let Some(u) = self.until {
            m.insert("until".into(), Json::Int(u as i64));
        }
        if let Some(a) = self.evidence_max_age_ms {
            m.insert("evidence_max_age".into(), Json::Int(a as i64));
        }
        if !self.dependency_capabilities.is_empty() {
            m.insert(
                "dependency_capabilities".into(),
                Json::Arr(
                    self.dependency_capabilities
                        .iter()
                        .map(|s| Json::str(s.clone()))
                        .collect(),
                ),
            );
        }
        if let Some(d) = &self.design_ref {
            m.insert("design_ref".into(), Json::str(d.clone()));
        }
        Json::Obj(m)
    }

    /// Decode `{until?, evidence_max_age?, dependency_capabilities[]?, design_ref?}`.
    pub fn from_json(j: &Json, path: &str) -> Result<ExpiryParams, DebtSchemaError> {
        Ok(ExpiryParams {
            until: opt_u64(j, "until", path)?,
            evidence_max_age_ms: opt_u64(j, "evidence_max_age", path)?,
            dependency_capabilities: str_arr(j, "dependency_capabilities", path)?,
            design_ref: opt_str(j, "design_ref", path)?,
        })
    }
}

impl DebtExpiry {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("condition", Json::str(self.condition.name())),
            ("params", self.params.to_json()),
        ])
    }

    /// Decode `{condition, params{…}}`.
    pub fn from_json(j: &Json, path: &str) -> Result<DebtExpiry, DebtSchemaError> {
        let cs = req_str(j, "condition", path)?;
        let condition = ExpiryKind::parse(&cs)
            .ok_or_else(|| bad(format!("{path}.condition: unknown expiry kind {cs}")))?;
        Ok(DebtExpiry {
            condition,
            params: ExpiryParams::from_json(req(j, "params", path)?, &format!("{path}.params"))?,
        })
    }
}

impl Revalidation {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        if !self.on.is_empty() {
            m.insert(
                "on".into(),
                Json::Arr(self.on.iter().map(|o| Json::str(o.name())).collect()),
            );
        }
        if let Some(a) = &self.action {
            m.insert("action".into(), Json::str(a.name()));
        }
        Json::Obj(m)
    }

    /// Decode `{on[]?, action?}`.
    pub fn from_json(j: &Json, path: &str) -> Result<Revalidation, DebtSchemaError> {
        let mut on = Vec::new();
        if let Some(Json::Arr(items)) = opt(j, "on") {
            for (i, v) in items.iter().enumerate() {
                let s = v
                    .as_str()
                    .ok_or_else(|| bad(format!("{path}.on[{i}] is not a string")))?;
                on.push(
                    RevalidationOn::parse(s)
                        .ok_or_else(|| bad(format!("{path}.on[{i}]: unknown trigger {s}")))?,
                );
            }
        }
        let action = match opt_str(j, "action", path)? {
            None => None,
            Some(s) => Some(
                RevalidationAction::parse(&s)
                    .ok_or_else(|| bad(format!("{path}.action: unknown action {s}")))?,
            ),
        };
        Ok(Revalidation { on, action })
    }
}

impl RemovalTest {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("kind".into(), Json::str(self.kind.name()));
        if let Some(t) = &self.template_ref {
            m.insert("template_ref".into(), Json::str(t.clone()));
        }
        if !self.beneficiaries.is_empty() {
            m.insert(
                "beneficiaries".into(),
                Json::Arr(
                    self.beneficiaries
                        .iter()
                        .map(|s| Json::str(s.clone()))
                        .collect(),
                ),
            );
        }
        if !self.probe_refs.is_empty() {
            m.insert(
                "probe_refs".into(),
                Json::Arr(
                    self.probe_refs
                        .iter()
                        .map(|s| Json::str(s.clone()))
                        .collect(),
                ),
            );
        }
        if let Some(s) = &self.scope_ref {
            m.insert("scope_ref".into(), Json::str(s.clone()));
        }
        if let Some(v) = &self.version_bound {
            m.insert("version_bound".into(), Json::str(v.clone()));
        }
        if let Some(c) = &self.conformance_suite_ref {
            m.insert("conformance_suite_ref".into(), Json::str(c.clone()));
        }
        if let Some(a) = &self.attestation_ref {
            m.insert("attestation_ref".into(), Json::str(a.clone()));
        }
        if let Some(c) = &self.criteria {
            m.insert("criteria".into(), Json::str(c.clone()));
        }
        if !self.supersedes_refs.is_empty() {
            m.insert(
                "supersedes_refs".into(),
                Json::Arr(
                    self.supersedes_refs
                        .iter()
                        .map(|s| Json::str(s.clone()))
                        .collect(),
                ),
            );
        }
        if let Some(d) = self.deadline {
            m.insert("deadline".into(), Json::Int(d as i64));
        }
        Json::Obj(m)
    }

    /// Decode `{kind, …payload members…}`.
    pub fn from_json(j: &Json, path: &str) -> Result<RemovalTest, DebtSchemaError> {
        let ks = req_str(j, "kind", path)?;
        let kind = RemovalTestKind::parse(&ks)
            .ok_or_else(|| bad(format!("{path}.kind: unknown removal-test kind {ks}")))?;
        Ok(RemovalTest {
            kind,
            template_ref: opt_str(j, "template_ref", path)?,
            beneficiaries: str_arr(j, "beneficiaries", path)?,
            probe_refs: str_arr(j, "probe_refs", path)?,
            scope_ref: opt_str(j, "scope_ref", path)?,
            version_bound: opt_str(j, "version_bound", path)?,
            conformance_suite_ref: opt_str(j, "conformance_suite_ref", path)?,
            attestation_ref: opt_str(j, "attestation_ref", path)?,
            criteria: opt_str(j, "criteria", path)?,
            supersedes_refs: str_arr(j, "supersedes_refs", path)?,
            deadline: opt_u64(j, "deadline", path)?,
        })
    }
}

impl RemovalVerdict {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("debt_ref".into(), Json::str(self.debt_ref.clone()));
        m.insert("kind".into(), Json::str(self.kind.name()));
        m.insert("verdict".into(), Json::str(self.verdict.name()));
        if let Some(r) = &self.reason {
            m.insert("reason".into(), Json::str(r.clone()));
        }
        m.insert("report_ref".into(), Json::str(self.report_ref.clone()));
        m.insert("settled_at".into(), Json::Int(self.settled_at as i64));
        Json::Obj(m)
    }

    /// Decode `{debt_ref, kind, verdict, reason?, report_ref, settled_at}`.
    pub fn from_json(j: &Json, path: &str) -> Result<RemovalVerdict, DebtSchemaError> {
        let vs = req_str(j, "verdict", path)?;
        let verdict = Verdict::parse(&vs)
            .ok_or_else(|| bad(format!("{path}.verdict: unknown verdict {vs}")))?;
        let ks = req_str(j, "kind", path)?;
        Ok(RemovalVerdict {
            debt_ref: req_str(j, "debt_ref", path)?,
            kind: RemovalTestKind::parse(&ks)
                .ok_or_else(|| bad(format!("{path}.kind: unknown removal-test kind {ks}")))?,
            verdict,
            reason: opt_str(j, "reason", path)?,
            report_ref: req_str(j, "report_ref", path)?,
            settled_at: opt_u64(j, "settled_at", path)?.unwrap_or(0),
        })
    }
}

impl OwnerRef {
    /// The canonical JSON — `{kind: principal|team, id, reach_via[]?}`.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert(
            "kind".into(),
            Json::str(if self.team { "team" } else { "principal" }),
        );
        m.insert("id".into(), Json::str(self.id.clone()));
        if !self.reach_via.is_empty() {
            m.insert(
                "reach_via".into(),
                Json::Arr(
                    self.reach_via
                        .iter()
                        .map(|s| Json::str(s.clone()))
                        .collect(),
                ),
            );
        }
        Json::Obj(m)
    }

    /// Decode `{kind, id, reach_via[]?}` — or a bare string (→ `principal{id}`).
    pub fn from_json(j: &Json, path: &str) -> Result<OwnerRef, DebtSchemaError> {
        match j {
            Json::Str(s) => Ok(OwnerRef::principal(s.clone())),
            Json::Obj(_) => {
                let ks = req_str(j, "kind", path)?;
                let team = match ks.as_str() {
                    "principal" => false,
                    "team" => true,
                    _ => {
                        return Err(bad(format!(
                            "{path}.kind: owner kind must be principal|team, got {ks}"
                        )))
                    }
                };
                Ok(OwnerRef {
                    team,
                    id: req_str(j, "id", path)?,
                    reach_via: str_arr(j, "reach_via", path)?,
                })
            }
            _ => Err(bad(format!("{path} is not an owner ref"))),
        }
    }
}

impl DebtRef {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("kind", Json::str(self.kind.clone())),
            ("ref", Json::str(self.reference.clone())),
        ])
    }

    /// Decode `{kind, ref}`.
    pub fn from_json(j: &Json, path: &str) -> Result<DebtRef, DebtSchemaError> {
        Ok(DebtRef {
            kind: req_str(j, "kind", path)?,
            reference: req_str(j, "ref", path)?,
        })
    }
}
