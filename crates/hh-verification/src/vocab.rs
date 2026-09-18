//! `vocab` — the closed sums the verification plane owns (spec §5f.1–§5f.4 §3;
//! ADR-0109…0117). Every sum is closed per dialect: unknown spellings refuse,
//! never coerce. `Detector` itself is *not* re-declared here — the canonical
//! sum is `hh_ontology::compliance::Detector` (CC7); this module re-exports it.

use hh_wire::Json;

pub use hh_hir::kinds::ChargedTo;
pub use hh_ontology::compliance::Detector;
pub use hh_ontology::eval::{EvidenceKind, LatticeValue, OracleClass, VerdictType};

// ── Claims (§5f.2 §3; ADR-0112 D1/D5) ─────────────────────────────────────────

/// `ClaimKind` — the closed claim-kind sum (ADR-0112 D1/D5). `unachievable` is
/// the honest-failure channel and is never a divergence by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ClaimKind {
    /// `observed` — the model claims it saw something.
    Observed,
    /// `effected` — the model claims it changed something.
    Effected,
    /// `verified` — the model claims a check passed.
    Verified,
    /// `achieved` — the model claims a goal/criterion is met.
    Achieved,
    /// `unachievable` — the model reports it cannot be done (honest failure).
    Unachievable,
    /// `progress` — the model claims forward progress on an item.
    Progress,
    /// `pending` — the model declares an item still open.
    Pending,
    /// `assumption` — the model declares an assumption it relied on.
    Assumption,
}

impl ClaimKind {
    /// The closed list (canonical order).
    pub const ALL: [ClaimKind; 8] = [
        ClaimKind::Observed,
        ClaimKind::Effected,
        ClaimKind::Verified,
        ClaimKind::Achieved,
        ClaimKind::Unachievable,
        ClaimKind::Progress,
        ClaimKind::Pending,
        ClaimKind::Assumption,
    ];

    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ClaimKind::Observed => "observed",
            ClaimKind::Effected => "effected",
            ClaimKind::Verified => "verified",
            ClaimKind::Achieved => "achieved",
            ClaimKind::Unachievable => "unachievable",
            ClaimKind::Progress => "progress",
            ClaimKind::Pending => "pending",
            ClaimKind::Assumption => "assumption",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<ClaimKind> {
        ClaimKind::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// `SubjectRef` — the closed subject sum (ADR-0112 D1). Binds to
/// `semantic_id`s, never surface names (T-LCD-10).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SubjectRef {
    /// `file{path}` — a workspace file.
    File(String),
    /// `effect{effect_id}` — an effect.
    Effect(String),
    /// `validator_target{ref}` — a validator's target.
    ValidatorTarget(String),
    /// `criterion{criterion_ref}` — an acceptance criterion (a
    /// `Goal.success_criteria` index/ref — never an ordinal alone).
    Criterion(String),
    /// `artifact{ref}` — an artifact (content or version ref).
    Artifact(String),
    /// `detached_process{process_ref}` — a detached process.
    DetachedProcess(String),
    /// `environment_state{selector}` — an environment-state selector.
    EnvironmentState(String),
    /// `progress_item{item_id}` — a progress-artifact item.
    ProgressItem(String),
    /// `run` — the run itself (the top-level completion claim's subject).
    Run,
}

impl SubjectRef {
    /// The kind tag.
    pub fn kind_tag(&self) -> &'static str {
        match self {
            SubjectRef::File(_) => "file",
            SubjectRef::Effect(_) => "effect",
            SubjectRef::ValidatorTarget(_) => "validator_target",
            SubjectRef::Criterion(_) => "criterion",
            SubjectRef::Artifact(_) => "artifact",
            SubjectRef::DetachedProcess(_) => "detached_process",
            SubjectRef::EnvironmentState(_) => "environment_state",
            SubjectRef::ProgressItem(_) => "progress_item",
            SubjectRef::Run => "run",
        }
    }

    /// The subject's ref (`run` has none — the run is the subject).
    pub fn ref_id(&self) -> Option<&str> {
        match self {
            SubjectRef::File(r)
            | SubjectRef::Effect(r)
            | SubjectRef::ValidatorTarget(r)
            | SubjectRef::Criterion(r)
            | SubjectRef::Artifact(r)
            | SubjectRef::DetachedProcess(r)
            | SubjectRef::EnvironmentState(r)
            | SubjectRef::ProgressItem(r) => Some(r),
            SubjectRef::Run => None,
        }
    }

    /// The canonical JSON (`{kind, ref?}`).
    pub fn to_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert("kind".to_string(), Json::str(self.kind_tag()));
        if let Some(r) = self.ref_id() {
            m.insert("ref".to_string(), Json::str(r));
        }
        Json::Obj(m)
    }
}

/// `extracted_by` — how the claim left the model's output (ADR-0112 D1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractedBy {
    /// `structured(surface_field_id)` — a compiled claim-surface field
    /// (confidence 1.0).
    Structured(String),
    /// `parsed(grammar_ref)` — parsed free text under the profile's grammar
    /// (capped confidence — never 1.0).
    Parsed(String),
    /// `judged(detector_ref)` — a judged extractor (R-2.7.2b; C2 — declared,
    /// never produced at C0).
    Judged(String),
}

impl ExtractedBy {
    /// The kind tag.
    pub fn kind_tag(&self) -> &'static str {
        match self {
            ExtractedBy::Structured(_) => "structured",
            ExtractedBy::Parsed(_) => "parsed",
            ExtractedBy::Judged(_) => "judged",
        }
    }

    /// The extraction confidence ceiling (ppm — 1_000_000 = 1.0). Structured
    /// fields are exact; parsed text is capped (`PARSED_CONFIDENCE_CAP_PPM`);
    /// judged extraction is never produced at C0 but carries the same cap.
    pub fn confidence_ceiling_ppm(&self) -> u64 {
        match self {
            ExtractedBy::Structured(_) => 1_000_000,
            ExtractedBy::Parsed(_) | ExtractedBy::Judged(_) => PARSED_CONFIDENCE_CAP_PPM,
        }
    }
}

/// The parsed/judged extraction confidence cap — 0.8 (OQ-free constant of the
/// claim surface; a parsed claim is never as strong as a structured field).
pub const PARSED_CONFIDENCE_CAP_PPM: u64 = 800_000;

/// `evidence_class` — the closed evidence-class sum (ADR-0115 D3). Orthogonal
/// to `AuthorityClass` (CF-080): an `external` document read by the kernel is
/// `measured` as to its bytes and `claimed` as to any assertion inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EvidenceClass {
    /// `measured` — kernel/environment facts (ledger events, validator
    /// verdicts, end-state reads, executable results, kernel-taken
    /// screenshots, read-only probe results).
    Measured,
    /// `reconciled` — `verification.claim.reconciled{agreement}` (R-2.7.2a is
    /// the sole producer).
    Reconciled,
    /// `claimed` — any AgentProcess output (messages, reasoning, self-reports,
    /// `risk_assessment`, hosted-participant claims).
    Claimed,
}

impl EvidenceClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EvidenceClass::Measured => "measured",
            EvidenceClass::Reconciled => "reconciled",
            EvidenceClass::Claimed => "claimed",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<EvidenceClass> {
        [
            EvidenceClass::Measured,
            EvidenceClass::Reconciled,
            EvidenceClass::Claimed,
        ]
        .into_iter()
        .find(|c| c.as_str() == s)
    }
}

// ── Reconciliation (§5f.2 §3; ADR-0112 D4/D5, ADR-0113) ───────────────────────

/// `agreement` — the four-valued reconciliation outcome (ADR-0112 D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agreement {
    /// `agree` — the claim matches its handles.
    Agree,
    /// `diverge{class}` — a typed divergence (a `diverge` cites ≥ 1 handle
    /// record on the record, never the claim's text).
    Diverge(DivergenceClass),
    /// `unverifiable` — no binding or no determinable handle value; never
    /// `agree`, never passes a `required` criterion.
    Unverifiable,
    /// `stale` — the handles moved past the claim's `at_seq`.
    Stale,
}

impl Agreement {
    /// The canonical spelling (a `diverge` renders `diverge{<class>}` in
    /// payloads via [`DivergenceClass::as_str`]).
    pub fn kind_tag(&self) -> &'static str {
        match self {
            Agreement::Agree => "agree",
            Agreement::Diverge(_) => "diverge",
            Agreement::Unverifiable => "unverifiable",
            Agreement::Stale => "stale",
        }
    }
}

/// `DivergenceClass` — the closed divergence taxonomy (ADR-0112 D5). D2/D3/D5/D6
/// are the ledger-only C0 subset; D1/D4/D7–D10 are C2 (R-2.7.2b).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DivergenceClass {
    /// D1 `phantom_observation` — a claimed observation with no authoritative
    /// record (C2 — needs world/probe checks).
    PhantomObservation,
    /// D2 `phantom_effect` — a claimed effect absent from `action.effect.*`.
    PhantomEffect,
    /// D3 `unverified_verification` — a claimed verification contradicted by
    /// the authoritative record (e.g. "tests pass" after a `fail` verdict).
    UnverifiedVerification,
    /// D4 `stale_belief` — the claimed state predates a measured change (C2).
    StaleBelief,
    /// D5 `contract_gap` — a `required` criterion unmet/unverifiable/unrun.
    ContractGap,
    /// D6 `open_effect_at_completion` — unresolved effects remain at a
    /// completion claim.
    OpenEffectAtCompletion,
    /// D7 `censored_evidence` — a claim hides a relevant record (C2).
    CensoredEvidence,
    /// D8 `progress_regression` — a claimed advance contradicts a measured
    /// regression (C2).
    ProgressRegression,
    /// D9 `evidence_inversion` — the cited evidence says the opposite (C2 —
    /// the deterministic arm fires mid-run).
    EvidenceInversion,
    /// D10 `no_progress_loop` — a repeated progress claim without measurable
    /// change (C2).
    NoProgressLoop,
}

impl DivergenceClass {
    /// The closed list (D-order).
    pub const ALL: [DivergenceClass; 10] = [
        DivergenceClass::PhantomObservation,
        DivergenceClass::PhantomEffect,
        DivergenceClass::UnverifiedVerification,
        DivergenceClass::StaleBelief,
        DivergenceClass::ContractGap,
        DivergenceClass::OpenEffectAtCompletion,
        DivergenceClass::CensoredEvidence,
        DivergenceClass::ProgressRegression,
        DivergenceClass::EvidenceInversion,
        DivergenceClass::NoProgressLoop,
    ];

    /// The canonical spelling (the D-code name).
    pub fn as_str(self) -> &'static str {
        match self {
            DivergenceClass::PhantomObservation => "phantom_observation",
            DivergenceClass::PhantomEffect => "phantom_effect",
            DivergenceClass::UnverifiedVerification => "unverified_verification",
            DivergenceClass::StaleBelief => "stale_belief",
            DivergenceClass::ContractGap => "contract_gap",
            DivergenceClass::OpenEffectAtCompletion => "open_effect_at_completion",
            DivergenceClass::CensoredEvidence => "censored_evidence",
            DivergenceClass::ProgressRegression => "progress_regression",
            DivergenceClass::EvidenceInversion => "evidence_inversion",
            DivergenceClass::NoProgressLoop => "no_progress_loop",
        }
    }

    /// The D-code (`d1`…`d10`).
    pub fn code(self) -> &'static str {
        match self {
            DivergenceClass::PhantomObservation => "d1",
            DivergenceClass::PhantomEffect => "d2",
            DivergenceClass::UnverifiedVerification => "d3",
            DivergenceClass::StaleBelief => "d4",
            DivergenceClass::ContractGap => "d5",
            DivergenceClass::OpenEffectAtCompletion => "d6",
            DivergenceClass::CensoredEvidence => "d7",
            DivergenceClass::ProgressRegression => "d8",
            DivergenceClass::EvidenceInversion => "d9",
            DivergenceClass::NoProgressLoop => "d10",
        }
    }

    /// Parse a canonical spelling or D-code; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<DivergenceClass> {
        DivergenceClass::ALL
            .into_iter()
            .find(|c| c.as_str() == s || c.code() == s)
    }

    /// Whether the class is in the C0 ledger-only subset (D2/D3/D5/D6 — the
    /// deterministic detectors of this item; the rest are C2's judged/probe
    /// detectors).
    pub fn is_c0(self) -> bool {
        matches!(
            self,
            DivergenceClass::PhantomEffect
                | DivergenceClass::UnverifiedVerification
                | DivergenceClass::ContractGap
                | DivergenceClass::OpenEffectAtCompletion
        )
    }
}

/// `ReconcileMode` — `ledger_only | probe` (ADR-0112 D4). `probe` is C2's —
/// declared, never produced at C0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcileMode {
    /// `ledger_only` — pure in `(claim, records ≤ until_seq, detector version)`.
    LedgerOnly,
    /// `probe` — dispatches `read_only ∧ closed-world` effects through the
    /// ordinary effect lifecycle (C2).
    Probe,
}

impl ReconcileMode {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ReconcileMode::LedgerOnly => "ledger_only",
            ReconcileMode::Probe => "probe",
        }
    }
}

/// `SeverityLevel` — the shared five-level severity sum (OQ-125 shared shape;
/// ADR-0114 D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SeverityLevel {
    /// `info` — first D10 hit.
    Info,
    /// `low` — D4/D7/D8.
    Low,
    /// `medium` — mid-run D2/D3/deterministic D9.
    Medium,
    /// `high` — D2/D3/D5 on the completion claim.
    High,
    /// `critical` — false completion with irreversible external effects claimed.
    Critical,
}

impl SeverityLevel {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SeverityLevel::Info => "info",
            SeverityLevel::Low => "low",
            SeverityLevel::Medium => "medium",
            SeverityLevel::High => "high",
            SeverityLevel::Critical => "critical",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<SeverityLevel> {
        [
            SeverityLevel::Info,
            SeverityLevel::Low,
            SeverityLevel::Medium,
            SeverityLevel::High,
            SeverityLevel::Critical,
        ]
        .into_iter()
        .find(|l| l.as_str() == s)
    }
}

/// `SourceRegister` — the per-register ownership of a `SeverityRecord`
/// (OQ-125 shared shape; ADR-0114 D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRegister {
    /// `execution_alignment` — this plane's register.
    ExecutionAlignment,
    /// `security` — §05g's register.
    Security,
    /// `effect` — §05a's register.
    Effect,
}

impl SourceRegister {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SourceRegister::ExecutionAlignment => "execution_alignment",
            SourceRegister::Security => "security",
            SourceRegister::Effect => "effect",
        }
    }
}

/// `Intervention` — the closed intervention sum with its strength order
/// (`annotate < feed_back < require_validator < hold < escalate < stop`;
/// ADR-0113 D8). Γ may tighten a floor, never loosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Intervention {
    /// `annotate` — record-only.
    Annotate,
    /// `feed_back` — a `ReconciliationNotice` as a `kernel_notice` D1 candidate.
    FeedBack,
    /// `require_validator` — run the bound validators as ordinary effects.
    RequireValidator,
    /// `hold` — the gate holds completion.
    Hold,
    /// `escalate` — escalate to a principal.
    Escalate,
    /// `stop` — stop the run.
    Stop,
}

impl Intervention {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Intervention::Annotate => "annotate",
            Intervention::FeedBack => "feed_back",
            Intervention::RequireValidator => "require_validator",
            Intervention::Hold => "hold",
            Intervention::Escalate => "escalate",
            Intervention::Stop => "stop",
        }
    }

    /// Parse; unknown spellings refuse.
    pub fn parse(s: &str) -> Option<Intervention> {
        [
            Intervention::Annotate,
            Intervention::FeedBack,
            Intervention::RequireValidator,
            Intervention::Hold,
            Intervention::Escalate,
            Intervention::Stop,
        ]
        .into_iter()
        .find(|i| i.as_str() == s)
    }

    /// Whether `self` is at least as strong as `floor` (the tighten/loosen
    /// comparison `validate_gamma` applies).
    pub fn at_least(self, floor: Intervention) -> bool {
        self >= floor
    }
}

/// `GateVerdict` — the gate's three-valued outcome (ADR-0113 D1).
#[derive(Debug, Clone, PartialEq)]
pub enum GateVerdict {
    /// `pass` — the completion may proceed.
    Pass,
    /// `hold{divergences, required_actions}` — completion held pending
    /// resolution (each hold consumes one `reconciliation.holds` unit — F4).
    Hold {
        /// The divergences that hold (≥ 1 — a hold always names its cause).
        divergences: Vec<DivergenceClass>,
        /// The required actions (`resolve_effect(id)`, `require_validator(refs)`).
        required_actions: Vec<String>,
    },
    /// `veto{invariant}` — a veto invariant tripped (success-with-veto path).
    Veto {
        /// The tripped invariant's name.
        invariant: String,
    },
}

impl GateVerdict {
    /// The kind tag.
    pub fn kind_tag(&self) -> &'static str {
        match self {
            GateVerdict::Pass => "pass",
            GateVerdict::Hold { .. } => "hold",
            GateVerdict::Veto { .. } => "veto",
        }
    }
}

/// `AuthoritativeHandle.kind` — the closed handle-kind sum (ADR-0112 D3).
/// Closed-schema handle values require the `environment` class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleKind {
    /// `ledger_event` — an event ref.
    LedgerEvent,
    /// `effect_state` — an effect's lifecycle state.
    EffectState,
    /// `validator_verdict` — a `verification.validator.verdict` record.
    ValidatorVerdict,
    /// `world_state` — a `action.world_state.*` record.
    WorldState,
    /// `environment_probe` — an environment probe result.
    EnvironmentProbe,
    /// `task_contract` — the task contract.
    TaskContract,
    /// `progress_artifact_version` — a progress-artifact version.
    ProgressArtifactVersion,
}

impl HandleKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            HandleKind::LedgerEvent => "ledger_event",
            HandleKind::EffectState => "effect_state",
            HandleKind::ValidatorVerdict => "validator_verdict",
            HandleKind::WorldState => "world_state",
            HandleKind::EnvironmentProbe => "environment_probe",
            HandleKind::TaskContract => "task_contract",
            HandleKind::ProgressArtifactVersion => "progress_artifact_version",
        }
    }
}

// ── Evidence + verdicts (§5f.1 §3; ADR-0109/0110/0111) ────────────────────────

/// `Freshness` — an `EvidenceRequirement`'s freshness arm (ADR-0109 D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// `any` — no freshness constraint.
    Any,
    /// `after_last_effect_on_scope` — produced after the last effect terminal
    /// event on the criterion's scope.
    AfterLastEffectOnScope,
    /// `after_seq(n)` — produced at seq > n.
    AfterSeq(u64),
}

impl Freshness {
    /// Whether a handle produced at `produced_at_seq` satisfies the freshness
    /// requirement given `last_effect_seq` on the scope (None = no effect).
    pub fn satisfied_by(&self, produced_at_seq: u64, last_effect_seq: Option<u64>) -> bool {
        match self {
            Freshness::Any => true,
            Freshness::AfterLastEffectOnScope => {
                last_effect_seq.is_none_or(|s| produced_at_seq > s)
            }
            Freshness::AfterSeq(n) => produced_at_seq > *n,
        }
    }
}

/// `Integrity` — an `EvidenceRequirement`'s integrity arm (ADR-0109 D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Integrity {
    /// `content_addressed` — the ref is a `ContentAddress`.
    ContentAddressed,
    /// `chain_verified` — the ref resolves inside the verified chain.
    ChainVerified,
}

impl Integrity {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Integrity::ContentAddressed => "content_addressed",
            Integrity::ChainVerified => "chain_verified",
        }
    }
}

/// `CriterionRole` — a `ValidatesRecord`'s `role` (ADR-0109 D2); also the
/// `Verdict.role` vocabulary (one sum, CC7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CriterionRole {
    /// `acceptance` — an acceptance criterion.
    Acceptance,
    /// `invariant` — a run invariant.
    Invariant,
    /// `precondition` — a precondition.
    Precondition,
    /// `postcondition` — a postcondition.
    Postcondition,
}

impl CriterionRole {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CriterionRole::Acceptance => "acceptance",
            CriterionRole::Invariant => "invariant",
            CriterionRole::Precondition => "precondition",
            CriterionRole::Postcondition => "postcondition",
        }
    }
}

/// `VerdictPhase` — the verdict's placement phase (ADR-0110 D3/D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictPhase {
    /// `local` — a local check (subject-bound, `charged_to = subject`).
    Local,
    /// `global` — a decision-point/global check.
    Global,
    /// `completion` — the completion gate.
    Completion,
    /// `instrument` — a Lab/instrument verdict (`charged_to = instrument`).
    Instrument,
}

impl VerdictPhase {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            VerdictPhase::Local => "local",
            VerdictPhase::Global => "global",
            VerdictPhase::Completion => "completion",
            VerdictPhase::Instrument => "instrument",
        }
    }
}

/// `Visibility` — a criterion's visibility (ADR-0109 D2/D3). `held_out` is
/// never the payload of `context.artefact.delivered` (`HeldOutLeak`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// `visible` — may be compiled to a `ContextItem` at `seal`.
    Visible,
    /// `held_out` — never delivered; executed by the instrument.
    HeldOut,
}

impl Visibility {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Visibility::Visible => "visible",
            Visibility::HeldOut => "held_out",
        }
    }
}

/// `CompletionPolicy` — a `TaskContract`'s completion policy (ADR-0109 D1/D5).
#[derive(Debug, Clone, PartialEq)]
pub enum CompletionPolicy {
    /// `all_required` — every `required` non-held-out criterion must pass.
    AllRequired,
    /// `weighted_threshold(t)` — a weighted threshold (ppm).
    WeightedThreshold(u64),
    /// `unverifiable(reason)` — requires `Goal.unverifiable_reason`; the run
    /// ends `succeeded_unverified`, never `succeeded`.
    Unverifiable(String),
}

/// `ThreeValued` — `pass | fail | inconclusive`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreeValued {
    /// `pass`.
    Pass,
    /// `fail`.
    Fail,
    /// `inconclusive` — never a pass.
    Inconclusive,
}

impl ThreeValued {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ThreeValued::Pass => "pass",
            ThreeValued::Fail => "fail",
            ThreeValued::Inconclusive => "inconclusive",
        }
    }
}

/// `VerdictValue` — the typed verdict value (a verdict is never a bare score —
/// ADR-0047 D2).
#[derive(Debug, Clone, PartialEq)]
pub enum VerdictValue {
    /// `bool`.
    Bool(bool),
    /// `graded` — a graded value in ppm (0…1_000_000).
    Graded(u64),
    /// `lattice{C,I,P,N}`.
    Lattice(LatticeValue),
    /// `three_valued`.
    ThreeValued(ThreeValued),
    /// `vector` — a structured vector (opaque members).
    Vector(Json),
}

impl VerdictValue {
    /// Whether the value is an affirmative (`pass`/`true`/`N`). Used only for
    /// the *local-check summary* — never as an authority or headline read.
    pub fn is_affirmative(&self) -> bool {
        match self {
            VerdictValue::Bool(b) => *b,
            VerdictValue::ThreeValued(ThreeValued::Pass) => true,
            VerdictValue::Lattice(LatticeValue::N) => true,
            _ => false,
        }
    }
}

/// `InconclusiveReason` — the closed inconclusive-reason sum (the
/// `CriticVerdict`/`Verdict` shared form; ADR-0115 D5, ADR-0116 D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InconclusiveReason {
    /// `missing_evidence` — an omitted/unavailable item the check needed
    /// (never `pass`, CF-245).
    MissingEvidence,
    /// `budget` — the budget starved the check (`budget_exhausted` ≠
    /// `oracle_failure`).
    Budget,
    /// `out_of_rubric` — the question is outside the declared rubric.
    OutOfRubric,
    /// `parse_failure` — the verdict could not be parsed.
    ParseFailure,
    /// `stale_evidence` — the evidence was stale (`EvidenceStale`).
    StaleEvidence,
    /// `unauthoritative` — evidence authority < `environment`.
    Unauthoritative,
    /// `unchecked_keyword` — a schema keyword outside the admitted OQ-219
    /// subset (the checker never guesses — T-LCD-15).
    UncheckedKeyword,
}

impl InconclusiveReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            InconclusiveReason::MissingEvidence => "missing_evidence",
            InconclusiveReason::Budget => "budget",
            InconclusiveReason::OutOfRubric => "out_of_rubric",
            InconclusiveReason::ParseFailure => "parse_failure",
            InconclusiveReason::StaleEvidence => "stale_evidence",
            InconclusiveReason::Unauthoritative => "unauthoritative",
            InconclusiveReason::UncheckedKeyword => "unchecked_keyword",
        }
    }
}

/// `OracleCause` — the closed `oracle_failure{cause}` sum (ADR-0047 D4 — an
/// outcome class, never a value).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OracleCause {
    /// `crash`.
    Crash,
    /// `timeout`.
    Timeout,
    /// `unparseable`.
    Unparseable,
    /// `non_finite`.
    NonFinite,
    /// `refused`.
    Refused,
}

impl OracleCause {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            OracleCause::Crash => "crash",
            OracleCause::Timeout => "timeout",
            OracleCause::Unparseable => "unparseable",
            OracleCause::NonFinite => "non_finite",
            OracleCause::Refused => "refused",
        }
    }
}

/// `VerdictStatus` — `decided | inconclusive{reason} | oracle_failure{cause}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictStatus {
    /// `decided` — the check ran and produced a value.
    Decided,
    /// `inconclusive{reason}` — the check ran but cannot decide (never a pass).
    Inconclusive(InconclusiveReason),
    /// `oracle_failure{cause}` — the oracle itself failed (an outcome class
    /// excluded from every denominator — ADR-0047 D4).
    OracleFailure(OracleCause),
}

impl VerdictStatus {
    /// The kind tag.
    pub fn kind_tag(&self) -> &'static str {
        match self {
            VerdictStatus::Decided => "decided",
            VerdictStatus::Inconclusive(_) => "inconclusive",
            VerdictStatus::OracleFailure(_) => "oracle_failure",
        }
    }
}

/// `Isolation` — a declaration's `isolation` arm (ADR-0110 D1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Isolation {
    /// `kernel` — runs in the kernel.
    Kernel,
    /// `sandbox(env_handle)` — runs the pinned payload in the sandbox/helper
    /// layer under an `EnvHandle`.
    Sandbox(String),
    /// `external` — runs out of the environment.
    External,
}

// ── Critics (§5f.4 §3; ADR-0115/0116/0117) ────────────────────────────────────

/// `CriticKind` — the closed critic-kind sum (ADR-0115 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CriticKind {
    /// `programmatic` — a deterministic programmatic critic.
    Programmatic,
    /// `judge` — a model judge (C2).
    Judge,
    /// `agentic_judge` — an agentic judge with probes (C2).
    AgenticJudge,
    /// `emulated` — an emulated pre-check (veto-only; C2).
    Emulated,
    /// `monitor` — an adversarial monitor (veto-only).
    Monitor,
}

impl CriticKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CriticKind::Programmatic => "programmatic",
            CriticKind::Judge => "judge",
            CriticKind::AgenticJudge => "agentic_judge",
            CriticKind::Emulated => "emulated",
            CriticKind::Monitor => "monitor",
        }
    }

    /// Whether the kind is model-based (the `JudgeNotIndependent`/
    /// `RedundantJudge` rules key on this).
    pub fn is_model_based(self) -> bool {
        matches!(self, CriticKind::Judge | CriticKind::AgenticJudge)
    }
}

/// `EvidenceMode` — the evidence modes a critic's bundle may draw on
/// (ADR-0115 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EvidenceMode {
    /// `ledger_view`.
    LedgerView,
    /// `end_state`.
    EndState,
    /// `artifacts`.
    Artifacts,
    /// `workspace_probe`.
    WorkspaceProbe,
    /// `screenshots`.
    Screenshots,
    /// `transcript`.
    Transcript,
    /// `reasoning`.
    Reasoning,
    /// `emulation`.
    Emulation,
}

impl EvidenceMode {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EvidenceMode::LedgerView => "ledger_view",
            EvidenceMode::EndState => "end_state",
            EvidenceMode::Artifacts => "artifacts",
            EvidenceMode::WorkspaceProbe => "workspace_probe",
            EvidenceMode::Screenshots => "screenshots",
            EvidenceMode::Transcript => "transcript",
            EvidenceMode::Reasoning => "reasoning",
            EvidenceMode::Emulation => "emulation",
        }
    }

    /// The `evidence_class` items of this mode carry by default.
    pub fn default_class(self) -> EvidenceClass {
        match self {
            EvidenceMode::Transcript | EvidenceMode::Reasoning => EvidenceClass::Claimed,
            _ => EvidenceClass::Measured,
        }
    }
}

/// `AdmissionMode` — the declared evidence-admission mode (ADR-0116 D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionMode {
    /// `principal_only` — items with authority ≥ `principal` plus
    /// kernel-rendered facts (the reviewer default).
    PrincipalOnly,
    /// `quarantined_external` — adds `environment`/`external`/`unverified`
    /// items rendered at ≤ `role_map(authority)`; the verdict inherits the
    /// taint and may only *raise* into Π.
    QuarantinedExternal,
    /// `all_labelled` — instrument critics.
    AllLabelled,
}

impl AdmissionMode {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            AdmissionMode::PrincipalOnly => "principal_only",
            AdmissionMode::QuarantinedExternal => "quarantined_external",
            AdmissionMode::AllLabelled => "all_labelled",
        }
    }

    /// Whether an item at `authority` is admitted under the mode
    /// (kernel-rendered facts are always admitted).
    pub fn admits(&self, authority: hh_provenance::authority::AuthorityClass) -> bool {
        use hh_provenance::authority::AuthorityClass as A;
        match self {
            AdmissionMode::PrincipalOnly => authority >= A::Principal || authority == A::Kernel,
            AdmissionMode::QuarantinedExternal | AdmissionMode::AllLabelled => true,
        }
    }
}

/// `Site` — where the critic runs (ADR-0115 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Site {
    /// `instrument` — across the scorer boundary, per run.
    Instrument,
    /// `runtime` — at the placement's decision point.
    Runtime,
}

impl Site {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Site::Instrument => "instrument",
            Site::Runtime => "runtime",
        }
    }
}

/// `CriticUse` — `report | gate` (ADR-0115 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CriticUse {
    /// `report` — feeds a `MetricValue`.
    Report,
    /// `gate` — feeds a gate (authorize / stop / verify / evolution).
    Gate,
}

impl CriticUse {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CriticUse::Report => "report",
            CriticUse::Gate => "gate",
        }
    }
}

/// `Grounding` — the derived grounding value (ADR-0115 D4). `ungrounded` ⇒
/// `oracle_failure`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Grounding {
    /// `ungrounded` — no cited items.
    Ungrounded,
    /// `claimed` — only `claimed` items cited.
    Claimed,
    /// `reconciled` — strongest cited class is `reconciled`.
    Reconciled,
    /// `measured` — strongest cited class is `measured`.
    Measured,
}

impl Grounding {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Grounding::Ungrounded => "ungrounded",
            Grounding::Claimed => "claimed",
            Grounding::Reconciled => "reconciled",
            Grounding::Measured => "measured",
        }
    }

    /// From an `evidence_class`.
    pub fn of(class: EvidenceClass) -> Grounding {
        match class {
            EvidenceClass::Measured => Grounding::Measured,
            EvidenceClass::Reconciled => Grounding::Reconciled,
            EvidenceClass::Claimed => Grounding::Claimed,
        }
    }
}

/// `SnapshotIndependence` (ADR-0116 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SnapshotIndependence {
    /// `same_snapshot` — never admissible for a non-exploratory use.
    SameSnapshot,
    /// `different_snapshot_same_family` — admissible for `report` and bounded
    /// `gate` with a mandatory `self_preference_check` (the weaker class is
    /// recorded on every verdict).
    DifferentSnapshotSameFamily,
    /// `different_family`.
    DifferentFamily,
}

impl SnapshotIndependence {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SnapshotIndependence::SameSnapshot => "same_snapshot",
            SnapshotIndependence::DifferentSnapshotSameFamily => "different_snapshot_same_family",
            SnapshotIndependence::DifferentFamily => "different_family",
        }
    }
}

/// `ContextIndependence` (ADR-0116 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContextIndependence {
    /// `same_context` — same-context critics are not critics.
    SameContext,
    /// `shared_transcript`.
    SharedTranscript,
    /// `fresh`.
    Fresh,
}

impl ContextIndependence {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ContextIndependence::SameContext => "same_context",
            ContextIndependence::SharedTranscript => "shared_transcript",
            ContextIndependence::Fresh => "fresh",
        }
    }
}

/// `CapabilityIndependence` (ADR-0116 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CapabilityIndependence {
    /// `shared_mutable`.
    SharedMutable,
    /// `shared_readonly`.
    SharedReadonly,
    /// `isolated_readonly`.
    IsolatedReadonly,
}

impl CapabilityIndependence {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CapabilityIndependence::SharedMutable => "shared_mutable",
            CapabilityIndependence::SharedReadonly => "shared_readonly",
            CapabilityIndependence::IsolatedReadonly => "isolated_readonly",
        }
    }
}

/// `ProvenanceIndependence` (ADR-0116 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProvenanceIndependence {
    /// `model_authored_rubric` — a model-authored rubric is `delegate` content
    /// and cannot define success (`RubricBelowDefinition`).
    ModelAuthoredRubric,
    /// `lab_rubric`.
    LabRubric,
    /// `sealed_rubric`.
    SealedRubric,
}

impl ProvenanceIndependence {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ProvenanceIndependence::ModelAuthoredRubric => "model_authored_rubric",
            ProvenanceIndependence::LabRubric => "lab_rubric",
            ProvenanceIndependence::SealedRubric => "sealed_rubric",
        }
    }
}

/// `Optimization` — the critic's optimization independence (ADR-0116 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Optimization {
    /// `in_loop_unbounded`.
    InLoopUnbounded,
    /// `in_loop_bounded`.
    InLoopBounded,
    /// `held_out`.
    HeldOut,
}

impl Optimization {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Optimization::InLoopUnbounded => "in_loop_unbounded",
            Optimization::InLoopBounded => "in_loop_bounded",
            Optimization::HeldOut => "held_out",
        }
    }
}

/// `AdversarialClass` (ADR-0116 D1/D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AdversarialClass {
    /// `untrusted_unmonitored` — exploratory only.
    UntrustedUnmonitored,
    /// `untrusted_monitored`.
    UntrustedMonitored,
    /// `trusted_weaker`.
    TrustedWeaker,
}

impl AdversarialClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            AdversarialClass::UntrustedUnmonitored => "untrusted_unmonitored",
            AdversarialClass::UntrustedMonitored => "untrusted_monitored",
            AdversarialClass::TrustedWeaker => "trusted_weaker",
        }
    }
}

/// `CalibrationStatus` — `active | expiring | expired` (ADR-0117 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationStatus {
    /// `active`.
    Active,
    /// `expiring`.
    Expiring,
    /// `expired` — a `gate` judge degrades; a `report` judge renders
    /// `exploratory`.
    Expired,
}

impl CalibrationStatus {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CalibrationStatus::Active => "active",
            CalibrationStatus::Expiring => "expiring",
            CalibrationStatus::Expired => "expired",
        }
    }
}

/// `ReferenceOracle` — a calibration's `reference_oracle` (ADR-0117 D1:
/// deterministic or `human_panel`, never a judge).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceOracle {
    /// `executable`.
    Executable,
    /// `end_state`.
    EndState,
    /// `trace_predicate`.
    TracePredicate,
    /// `human_panel`.
    HumanPanel,
}

impl ReferenceOracle {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ReferenceOracle::Executable => "executable",
            ReferenceOracle::EndState => "end_state",
            ReferenceOracle::TracePredicate => "trace_predicate",
            ReferenceOracle::HumanPanel => "human_panel",
        }
    }
}

// ── Status/stratum values (ADR-0113 (e) — the named constants) ────────────────

/// `lifecycle.run.finished.status` value for the unverifiable completion
/// policy (`CompletionPolicy::Unverifiable`) — rendered `n/a{no_detector}`
/// for headline capability, never success (ADR-0045; CF-315).
pub const STATUS_SUCCEEDED_UNVERIFIED: &str = "succeeded_unverified";

/// The `scored` stratum for an honest `unachievable` failure (F5; ADR-0161 D5 —
/// no new enum value).
pub const STRATUM_FAILED_HONESTLY: &str = "failed_honestly";

/// The `scored` stratum for a `budget_exhausted{reconciliation.holds}` stop
/// (F4; ADR-0161 D5).
pub const STRATUM_UNRECONCILED_CLAIMS: &str = "unreconciled_claims";

/// The `false_completion` veto invariant's registered name (ADR-0047 D5 as
/// amended; AC-R-2.7.2a-9).
pub const VETO_FALSE_COMPLETION: &str = "false_completion";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_sums_round_trip_and_refuse_unknown() {
        assert_eq!(ClaimKind::ALL.len(), 8);
        assert_eq!(ClaimKind::parse("achieved"), Some(ClaimKind::Achieved));
        assert_eq!(ClaimKind::parse("done"), None);
        assert_eq!(DivergenceClass::ALL.len(), 10);
        assert_eq!(
            DivergenceClass::parse("d5"),
            Some(DivergenceClass::ContractGap)
        );
        assert_eq!(
            DivergenceClass::parse("phantom_effect"),
            Some(DivergenceClass::PhantomEffect)
        );
        assert!(DivergenceClass::PhantomEffect.is_c0());
        assert!(!DivergenceClass::PhantomObservation.is_c0());
        assert_eq!(
            EvidenceClass::parse("measured"),
            Some(EvidenceClass::Measured)
        );
        assert_eq!(EvidenceClass::parse("judged"), None);
        assert!(OracleClass::Executable.is_c0_headline());
        assert!(!OracleClass::Judge.is_c0_headline());
        assert_eq!(OracleClass::Judge.detector(), Detector::Judged);
        assert!(Intervention::Hold.at_least(Intervention::Annotate));
        assert!(!Intervention::Annotate.at_least(Intervention::Hold));
    }

    #[test]
    fn admission_modes_admit_per_the_declared_floor() {
        use hh_provenance::authority::AuthorityClass as A;
        assert!(AdmissionMode::PrincipalOnly.admits(A::Kernel));
        assert!(AdmissionMode::PrincipalOnly.admits(A::Principal));
        assert!(!AdmissionMode::PrincipalOnly.admits(A::External));
        assert!(!AdmissionMode::PrincipalOnly.admits(A::Unverified));
        assert!(AdmissionMode::QuarantinedExternal.admits(A::Unverified));
        assert!(AdmissionMode::AllLabelled.admits(A::Unverified));
    }
}
