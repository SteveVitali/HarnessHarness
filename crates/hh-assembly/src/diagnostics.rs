//! The closed `AssemblyDiagnostic` taxonomy (§3.3.8; ADR-0148 / WS-J1 §6.3; the ADR-0143
//! 7-L2 `BenchmarkConditionedRule` code). Every rejection path emits a *typed* diagnostic —
//! a code from the closed table below, a JSON-pointer `path`, the `source_layer` when the
//! section was composed from layers, the `subject`, the `stage` that raised it, a `detail`
//! `Text{owner = kernel}` leaf, a `remedy` spelling and the owning ADR. `C-INT-1
//! UncodedRejection` exists to make an *uncoded* rejection a defect — it is never emitted
//! for ordinary invalid input (AC-CC-11's corpus property).

use hh_hir::leaves::Text;
use hh_hir::HirError;
use hh_provenance::ProvenanceRecord;

/// `severity ∈ {error, warning, info}` (§3.3.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Blocking — the document is not admissible.
    Error,
    /// Non-blocking finding.
    Warning,
    /// Informational (`DenyListNoop`, derived-data notes).
    Info,
}

impl Severity {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

/// `stage ∈ {desugar, compose, resolve, validate:1..7, link, lower, instantiate, resume}`
/// (§3.3.8 — the stage that produced the diagnostic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// Desugaring (host-surface → assembly; the assembler's front end).
    Desugar,
    /// `compose` (Stage 3 — layered merge).
    Compose,
    /// `resolve` (pinning/substitution → seal).
    Resolve,
    /// `validate_assembly` stage `n` (1–7).
    Validate(u8),
    /// `link` (§3.2 stage 1 — consumes the sealed definition).
    Link,
    /// `lower` (§3.2 stage 2).
    Lower,
    /// `instantiate`.
    Instantiate,
    /// `verify_resume`.
    Resume,
}

impl Stage {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Desugar => "desugar",
            Stage::Compose => "compose",
            Stage::Resolve => "resolve",
            Stage::Validate(1) => "validate:1",
            Stage::Validate(2) => "validate:2",
            Stage::Validate(3) => "validate:3",
            Stage::Validate(4) => "validate:4",
            Stage::Validate(5) => "validate:5",
            Stage::Validate(6) => "validate:6",
            Stage::Validate(7) => "validate:7",
            Stage::Validate(_) => "validate:?",
            Stage::Link => "link",
            Stage::Lower => "lower",
            Stage::Instantiate => "instantiate",
            Stage::Resume => "resume",
        }
    }
}

/// The closed diagnostic code set (ADR-0148 + §3.3.8 + the ADR-0143 L2 code). Interior
/// numbering inside each family is this crate's assignment (ADR-0240); the codes the
/// spec pins by number — `C-COMP-3`, `C-REF-5`, `C-REF-6`, `C-PARAM-5`, `C-CLASS-6`,
/// `C-INT-1` — are assigned exactly as named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    // ── load ────────────────────────────────────────────────────────────────────
    /// `C-LOAD-1 ParseError` — malformed bytes / a member that fails its schema check.
    LoadParse,
    /// `C-LOAD-2 UnknownDialect` — the `dialect` member is not the expected one.
    LoadDialect,
    /// `C-LOAD-3 UnknownKey` — a non-`ext` member outside the closed grammar.
    LoadUnknownKey,
    // ── compose (the compose op is Stage 3 — registered now, produced then) ───────
    /// `C-COMP-1 AuthorityViolation` — a layer widens authority.
    CompAuthorityViolation,
    /// `C-COMP-2 LayerConflict` — equal-precedence layers / two sources set one path.
    CompLayerConflict,
    /// `C-COMP-3 LayerProvenanceMissing` — validate(1): a layered member lacks its
    /// `LayerProvenance` (or names an undeclared `layer_id`). **S1.9-owned.**
    CompLayerProvenanceMissing,
    /// `C-COMP-4 ForbiddenBelow` — a lower-precedence layer sets a `forbidden-below` path.
    CompForbiddenBelow,
    // ── resolve ─────────────────────────────────────────────────────────────────
    /// `C-REF-1 UnresolvedRef` — a slot variant / entity ref that resolves to nothing.
    RefUnresolved,
    /// `C-REF-2 AmbiguousVersion` — a selector matching more than one admissible head.
    RefAmbiguous,
    /// `C-REF-3 NameCollision` — the registry reports a contested name at resolve.
    RefNameCollision,
    /// `C-REF-4 StaleIndex` — a pinned closure containing a revoked/stale head
    /// (`execute` mode; ADR-0037).
    RefStaleIndex,
    /// `C-REF-5 DenyListNoop` — a deny-list selector whose deny list never bites (info;
    /// the spec-pinned number).
    RefDenyListNoop,
    /// `C-REF-6 SnapshotDrift` — the snapshot has moved since `resolve` (the spec-pinned
    /// number; §6.1's drift check).
    RefSnapshotDrift,
    // ── class conformance (validate:2) ───────────────────────────────────────────
    /// `C-CLASS-1 UnknownClass` — a slot key no catalog class owns.
    ClassUnknown,
    /// `C-CLASS-2 VariantClassMismatch` — the bound variant's `class_ref` ≠ the slot's
    /// class.
    ClassVariantMismatch,
    /// `C-CLASS-3 CardinalityViolation` — the slot's binding count breaks the class's
    /// `cardinality`.
    ClassCardinality,
    /// `C-CLASS-4 ContractMalformed` — a `ClassRecord.contract` member is missing the
    /// operations/inputs/outputs/invariants/failure-modes shape (AC-CC-01's static half).
    ClassContractMalformed,
    /// `C-CLASS-5 RequiredInputsMissing` — `required_inputs ⊉ {ModelProfile,
    /// ResourceAccount}` (AC-CC-02 / T-LCD-08).
    ClassRequiredInputs,
    /// `C-CLASS-6 SlotsOnHosted` — a `hosted` `AgentProcess` carrying slot bindings /
    /// `ComponentVariantRef`s (the spec-pinned number; T-LCD-15).
    ClassSlotsOnHosted,
    /// `C-CLASS-7 DialectIncompatible` — the class/variant record's dialect range
    /// excludes the document dialect.
    ClassDialectIncompatible,
    // ── parameter space (validate:3; stage-2's unknown-param lands here too) ──────
    /// `C-PARAM-1 UnknownParameter` — a `values` key, a `$param:` target or a slot-param
    /// name no declared schema covers.
    ParamUnknown,
    /// `C-PARAM-2 MissingDomain` — a `sweepable` parameter without a `domain`.
    ParamMissingDomain,
    /// `C-PARAM-3 UndeclaredParamRef` — a `$param:<id>` binding form for an undeclared id.
    ParamUndeclaredRef,
    /// `C-PARAM-4 UnusedParameter` — a declared parameter never consumed.
    ParamUnused,
    /// `C-PARAM-5 DefaultedBudgetRelevant` — a `budget_relevant` parameter filled only by
    /// its default (warning; the spec-pinned number).
    ParamDefaultedBudgetRelevant,
    // ── constraints (validate:4) ─────────────────────────────────────────────────
    /// `C-CONS-1 ConstraintViolation` — a `requires`/`conflicts`/`implies`/`range`/
    /// `authority_cap` constraint the document breaks.
    ConsViolation,
    // ── HIR kernel (validate:5 — the ADR-0016/0033 set mirrored 1:1) ─────────────
    /// `C-KERN-<HirError variant>` — e.g. `C-KERN-SchemaViolation`.
    Kern(&'static str),
    // ── profile compatibility (6a/6b — C1; registered now) ───────────────────────
    /// `C-PROF-1 ProfileIncompatible` (6a).
    ProfIncompatible,
    /// `C-PROF-2 UnexpressibleSurface` (6b).
    ProfUnexpressible,
    /// `C-PROF-3 ProfilePinnedAcrossProfiles` — a `non_portable` definition used across
    /// the profile factor (plan stage; §3.3.2 `profile_binding` note).
    ProfPinnedAcrossProfiles,
    // ── LCD static checks (validate:7) ───────────────────────────────────────────
    /// `C-LCD-1 HostingEdge` — a hosting edge in the sealed definition (7-L4;
    /// `hosting_edges = []` at C0).
    LcdHostingEdge,
    /// `C-LCD-2 IdentityIncludesSurface` — a semantic projection that carries a
    /// surface/provenance member (7-T10).
    LcdIdentityIncludesSurface,
    /// `C-LCD-3 InheritanceContract` — a class contract expressed by inheritance (7-T12).
    LcdInheritanceContract,
    /// `C-LCD-4 BenchmarkConditionedRule` — a `suite_id`/`task_id`/`foreign_id`/split/
    /// family predicate reference anywhere in the definition (7-L2; ADR-0143 D2 —
    /// **error, never warning**).
    LcdBenchmarkConditionedRule,
    // ── instantiate ──────────────────────────────────────────────────────────────
    /// `C-LOC-1 LocalityUnsupported` — the variant's `implementation.placement` is not
    /// bindable in this runtime (S1: in-process only).
    LocUnsupported,
    /// `C-TRUST-1 TrustDenied` — the runtime refuses the record's trust posture.
    TrustDenied,
    // ── secrets ──────────────────────────────────────────────────────────────────
    /// `C-SEC-1 InlineSecret` — an inline secret where a channel name (`$secret:<name>`)
    /// is required.
    SecInlineSecret,
    // ── link ─────────────────────────────────────────────────────────────────────
    /// `C-LINK-1 UnboundSlot` — an unpinned selector reaches the compiler's stage 1.
    LinkUnboundSlot,
    /// `C-LINK-2 NoProfile` — no bound profile and no `fallback_profile` with a dated
    /// debt hypothesis.
    LinkNoProfile,
    // ── plan / resume / internal ─────────────────────────────────────────────────
    /// `C-PLAN-1 PlanRefusal` — the assembly service's `plan` refuses (§6; registered).
    PlanRefusal,
    /// `C-RES-1 Incompatible` — `verify_resume` refuses.
    ResIncompatible,
    /// `C-INT-1 UncodedRejection` — a rejection that reached the caller with no code:
    /// a **defect**, never a normal fallback (the spec-pinned number).
    IntUncodedRejection,
}

impl Code {
    /// The canonical `C-…` spelling.
    pub fn code(self) -> String {
        use Code::*;
        match self {
            LoadParse => "C-LOAD-1".into(),
            LoadDialect => "C-LOAD-2".into(),
            LoadUnknownKey => "C-LOAD-3".into(),
            CompAuthorityViolation => "C-COMP-1".into(),
            CompLayerConflict => "C-COMP-2".into(),
            CompLayerProvenanceMissing => "C-COMP-3".into(),
            CompForbiddenBelow => "C-COMP-4".into(),
            RefUnresolved => "C-REF-1".into(),
            RefAmbiguous => "C-REF-2".into(),
            RefNameCollision => "C-REF-3".into(),
            RefStaleIndex => "C-REF-4".into(),
            RefDenyListNoop => "C-REF-5".into(),
            RefSnapshotDrift => "C-REF-6".into(),
            ClassUnknown => "C-CLASS-1".into(),
            ClassVariantMismatch => "C-CLASS-2".into(),
            ClassCardinality => "C-CLASS-3".into(),
            ClassContractMalformed => "C-CLASS-4".into(),
            ClassRequiredInputs => "C-CLASS-5".into(),
            ClassSlotsOnHosted => "C-CLASS-6".into(),
            ClassDialectIncompatible => "C-CLASS-7".into(),
            ParamUnknown => "C-PARAM-1".into(),
            ParamMissingDomain => "C-PARAM-2".into(),
            ParamUndeclaredRef => "C-PARAM-3".into(),
            ParamUnused => "C-PARAM-4".into(),
            ParamDefaultedBudgetRelevant => "C-PARAM-5".into(),
            ConsViolation => "C-CONS-1".into(),
            Kern(v) => format!("C-KERN-{v}"),
            ProfIncompatible => "C-PROF-1".into(),
            ProfUnexpressible => "C-PROF-2".into(),
            ProfPinnedAcrossProfiles => "C-PROF-3".into(),
            LcdHostingEdge => "C-LCD-1".into(),
            LcdIdentityIncludesSurface => "C-LCD-2".into(),
            LcdInheritanceContract => "C-LCD-3".into(),
            LcdBenchmarkConditionedRule => "C-LCD-4".into(),
            LocUnsupported => "C-LOC-1".into(),
            TrustDenied => "C-TRUST-1".into(),
            SecInlineSecret => "C-SEC-1".into(),
            LinkUnboundSlot => "C-LINK-1".into(),
            LinkNoProfile => "C-LINK-2".into(),
            PlanRefusal => "C-PLAN-1".into(),
            ResIncompatible => "C-RES-1".into(),
            IntUncodedRejection => "C-INT-1".into(),
        }
    }

    /// The diagnostic's named failure mode.
    pub fn name(self) -> &'static str {
        use Code::*;
        match self {
            LoadParse => "ParseError",
            LoadDialect => "UnknownDialect",
            LoadUnknownKey => "UnknownKey",
            CompAuthorityViolation => "AuthorityViolation",
            CompLayerConflict => "LayerConflict",
            CompLayerProvenanceMissing => "LayerProvenanceMissing",
            CompForbiddenBelow => "ForbiddenBelow",
            RefUnresolved => "UnresolvedRef",
            RefAmbiguous => "AmbiguousVersion",
            RefNameCollision => "NameCollision",
            RefStaleIndex => "StaleIndex",
            RefDenyListNoop => "DenyListNoop",
            RefSnapshotDrift => "SnapshotDrift",
            ClassUnknown => "UnknownClass",
            ClassVariantMismatch => "VariantClassMismatch",
            ClassCardinality => "CardinalityViolation",
            ClassContractMalformed => "ContractMalformed",
            ClassRequiredInputs => "RequiredInputsMissing",
            ClassSlotsOnHosted => "SlotsOnHosted",
            ClassDialectIncompatible => "DialectIncompatible",
            ParamUnknown => "UnknownParameter",
            ParamMissingDomain => "MissingDomain",
            ParamUndeclaredRef => "UndeclaredParamRef",
            ParamUnused => "UnusedParameter",
            ParamDefaultedBudgetRelevant => "DefaultedBudgetRelevant",
            ConsViolation => "ConstraintViolation",
            Kern(_) => "KernelError",
            ProfIncompatible => "ProfileIncompatible",
            ProfUnexpressible => "UnexpressibleSurface",
            ProfPinnedAcrossProfiles => "ProfilePinnedAcrossProfiles",
            LcdHostingEdge => "HostingEdge",
            LcdIdentityIncludesSurface => "IdentityIncludesSurface",
            LcdInheritanceContract => "InheritanceContract",
            LcdBenchmarkConditionedRule => "BenchmarkConditionedRule",
            LocUnsupported => "LocalityUnsupported",
            TrustDenied => "TrustDenied",
            SecInlineSecret => "InlineSecret",
            LinkUnboundSlot => "UnboundSlot",
            LinkNoProfile => "NoProfile",
            PlanRefusal => "PlanRefusal",
            ResIncompatible => "Incompatible",
            IntUncodedRejection => "UncodedRejection",
        }
    }

    /// Every registered code (the closed table — AC-CC-11's fixture battery iterates it;
    /// `C-KERN-*` lists the `HirError` variant spellings it mirrors 1:1).
    pub fn all() -> Vec<Code> {
        use Code::*;
        let mut v = vec![
            LoadParse,
            LoadDialect,
            LoadUnknownKey,
            CompAuthorityViolation,
            CompLayerConflict,
            CompLayerProvenanceMissing,
            CompForbiddenBelow,
            RefUnresolved,
            RefAmbiguous,
            RefNameCollision,
            RefStaleIndex,
            RefDenyListNoop,
            RefSnapshotDrift,
            ClassUnknown,
            ClassVariantMismatch,
            ClassCardinality,
            ClassContractMalformed,
            ClassRequiredInputs,
            ClassSlotsOnHosted,
            ClassDialectIncompatible,
            ParamUnknown,
            ParamMissingDomain,
            ParamUndeclaredRef,
            ParamUnused,
            ParamDefaultedBudgetRelevant,
            ConsViolation,
            ProfIncompatible,
            ProfUnexpressible,
            ProfPinnedAcrossProfiles,
            LcdHostingEdge,
            LcdIdentityIncludesSurface,
            LcdInheritanceContract,
            LcdBenchmarkConditionedRule,
            LocUnsupported,
            TrustDenied,
            SecInlineSecret,
            LinkUnboundSlot,
            LinkNoProfile,
            PlanRefusal,
            ResIncompatible,
            IntUncodedRejection,
        ];
        for k in KERN_VARIANTS {
            v.push(Kern(k));
        }
        v
    }
}

/// The `C-KERN-*` mirror of the `HirError` set (ADR-0148: ADR-0016/0033 1:1).
pub const KERN_VARIANTS: &[&str] = &[
    "UnknownKind",
    "DialectUnsupported",
    "UnresolvedRef",
    "CycleDetected",
    "EffectUncovered",
    "AuthorityWidening",
    "BudgetExceedsParent",
    "ConditionedRuleIncomplete",
    "NonCanonicalInput",
    "OpaqueWithoutInterface",
    "UnexpressibleSurface",
    "UnclassifiedKind",
    "MissingProvenance",
    "AuthorityExceedsOrigin",
    "TextAboveExternal",
    "TaintedAboveExternal",
    "IllegitimateEndorsement",
    "ScopeCeilingExceeded",
    "MigrationLoss",
    "DialectIncompatible",
    "SchemaViolation",
];

/// The `C-KERN-*` mirror — one code per `HirError` variant, 1:1 (ADR-0148).
pub fn kern_code(e: &HirError) -> Code {
    use HirError::*;
    Code::Kern(match e {
        UnknownKind { .. } => "UnknownKind",
        DialectUnsupported { .. } => "DialectUnsupported",
        UnresolvedRef { .. } => "UnresolvedRef",
        CycleDetected { .. } => "CycleDetected",
        EffectUncovered { .. } => "EffectUncovered",
        AuthorityWidening { .. } => "AuthorityWidening",
        BudgetExceedsParent { .. } => "BudgetExceedsParent",
        ConditionedRuleIncomplete { .. } => "ConditionedRuleIncomplete",
        NonCanonicalInput { .. } => "NonCanonicalInput",
        OpaqueWithoutInterface { .. } => "OpaqueWithoutInterface",
        UnexpressibleSurface { .. } => "UnexpressibleSurface",
        UnclassifiedKind { .. } => "UnclassifiedKind",
        MissingProvenance { .. } => "MissingProvenance",
        AuthorityExceedsOrigin { .. } => "AuthorityExceedsOrigin",
        TextAboveExternal { .. } => "TextAboveExternal",
        TaintedAboveExternal { .. } => "TaintedAboveExternal",
        IllegitimateEndorsement { .. } => "IllegitimateEndorsement",
        ScopeCeilingExceeded { .. } => "ScopeCeilingExceeded",
        MigrationLoss { .. } => "MigrationLoss",
        DialectIncompatible { .. } => "DialectIncompatible",
        SchemaViolation { .. } => "SchemaViolation",
    })
}

/// `AssemblyDiagnostic{code, class, severity, path, source_layer?, subject, stage,
/// detail: Text{owner = kernel}, remedy, owner_adr}` (§3.3.8; ADR-0148).
#[derive(Debug, Clone)]
pub struct AssemblyDiagnostic {
    /// The closed code (`C-…`).
    pub code: Code,
    /// The component class implicated, when the diagnostic is class-scoped.
    pub class: Option<String>,
    /// `error | warning | info`.
    pub severity: Severity,
    /// The JSON-pointer path into the document (`/assembly/slots/…`).
    pub path: String,
    /// The layer that authored the offending member, on composed documents.
    pub source_layer: Option<String>,
    /// The diagnosed subject (slot key, param id, ref coordinate, …).
    pub subject: String,
    /// The producing stage.
    pub stage: Stage,
    /// `detail: Text{owner = "kernel"}` — the typed leaf, never a bare string.
    pub detail: Text,
    /// The remedy spelling.
    pub remedy: String,
    /// The owning ADR (`ADR-0148`, `ADR-0143`, `ADR-0240`, …).
    pub owner_adr: String,
}

/// The kernel-owned `Text` leaf a diagnostic's `detail` carries (`owner = "kernel"`,
/// minted `external` authority — diagnostics are kernel output, never authored content).
pub fn detail_text(detail: impl Into<String>, kernel: &ProvenanceRecord) -> Text {
    Text::new(detail.into(), "kernel", kernel.clone())
}

/// A `n/a` reason class on a `StageOutcome` (§3.3.8: `n/a{not_run}` — the stage belongs
/// to a later stage's scope; `n/a{class}` — the subject's class makes the stage
/// inapplicable, e.g. hosted definitions at stages 2/6/7-opacity).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NaReason {
    /// The stage is staged later (`not_run`).
    NotRun,
    /// The subject class makes the stage inapplicable (`class`).
    Class,
}

impl NaReason {
    /// The canonical spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            NaReason::NotRun => "not_run",
            NaReason::Class => "class",
        }
    }
}

/// One stage's outcome in a `ValidationReport.stages` (§3.3.8 — every stage *runs and
/// reports*; `validate_assembly` is never fail-fast).
#[derive(Debug, Clone, PartialEq)]
pub struct StageOutcome {
    /// The stage number (1–7; `6` covers 6a/6b).
    pub stage: u8,
    /// `true` when the stage ran.
    pub ran: bool,
    /// The `n/a` reason when `ran = false`.
    pub na: Option<NaReason>,
    /// Error / warning / info counts this stage produced.
    pub errors: usize,
    /// Warning count.
    pub warnings: usize,
    /// Info count.
    pub infos: usize,
}

impl StageOutcome {
    /// A stage that ran.
    pub fn ran(stage: u8) -> StageOutcome {
        StageOutcome {
            stage,
            ran: true,
            na: None,
            errors: 0,
            warnings: 0,
            infos: 0,
        }
    }

    /// A stage reported `n/a{reason}`.
    pub fn not_applicable(stage: u8, na: NaReason) -> StageOutcome {
        StageOutcome {
            stage,
            ran: false,
            na: Some(na),
            errors: 0,
            warnings: 0,
            infos: 0,
        }
    }
}

/// The report `derived` block (§3.3.8): `opacity_ratio?`, `identity_stability`,
/// `hosting_edges`, `benchmark_conditioned_rules[]`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DerivedResults {
    /// The 7-O opacity report (counts by authority class; `None` when 7-O is `n/a`).
    pub opacity: Option<OpacitySummary>,
    /// 7-T10: `true` when recomputing the semantic identity over the surface-free
    /// projection reproduces the definition's `semantic_id`.
    pub identity_stability: Option<bool>,
    /// 7-L4: the definition's hosting edges — `[]` at C0 (any edge is `C-LCD-1`).
    pub hosting_edges: Vec<String>,
    /// 7-L2: the benchmark-conditioned findings (each is a `C-LCD-4` error).
    pub benchmark_conditioned_rules: Vec<String>,
}

/// The opacity partition (7-O; `hh-provenance`'s `OpacityReport` rendered as data):
/// per-`AuthorityClass` leaf counts plus the opaque share.
#[derive(Debug, Clone, PartialEq)]
pub struct OpacitySummary {
    /// `authority_class → leaf count`.
    pub by_class: std::collections::BTreeMap<String, usize>,
    /// Total leaves counted.
    pub total: usize,
    /// `opaque_without_interface + opaque_unowned` leaves over `total` (the opacity
    /// ratio AC-CC-04 reports).
    pub opaque_ratio_num: usize,
}

/// The `ValidationReport` (§3.3.8): `{status, diagnostics[], derived, stages}`.
#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    /// `pass | pass_with_warnings | fail`.
    pub status: ReportStatus,
    /// Every diagnostic every stage produced — a *complete* report, never fail-fast.
    pub diagnostics: Vec<AssemblyDiagnostic>,
    /// The derived results block.
    pub derived: DerivedResults,
    /// Per-stage outcomes (`ran | n/a{reason}` + counts).
    pub stages: Vec<StageOutcome>,
}

/// `ValidationReport.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReportStatus {
    /// Zero diagnostics at error severity.
    #[default]
    Pass,
    /// No errors; at least one warning.
    PassWithWarnings,
    /// At least one error.
    Fail,
}

impl ValidationReport {
    /// Recompute `status` and the per-stage counts from `diagnostics`.
    pub fn finalize(&mut self) {
        let errors = self
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        let warnings = self
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .count();
        self.status = if errors > 0 {
            ReportStatus::Fail
        } else if warnings > 0 {
            ReportStatus::PassWithWarnings
        } else {
            ReportStatus::Pass
        };
        for outcome in &mut self.stages {
            let stage = outcome.stage;
            let count = |sev: Severity| {
                self.diagnostics
                    .iter()
                    .filter(|d| {
                        matches!(d.stage, Stage::Validate(n) if n == stage) && d.severity == sev
                    })
                    .count()
            };
            outcome.errors = count(Severity::Error);
            outcome.warnings = count(Severity::Warning);
            outcome.infos = count(Severity::Info);
        }
    }
}
