//! The compiler's closed failure set (§3.2.10): every refusal is a typed
//! [`CompileError`] — an error with non-zero exit, never a warning, never a silent
//! fallback. `AssemblyDiagnostic`s from `validate_assembly`/`link_precheck` ride inside
//! the variants that carry them so the `C-*` codes reach the caller intact.

use hh_assembly::AssemblyDiagnostic;

/// `LinkError{unbound_slot | version_conflict | missing_debt_record | unknown_target}`
/// (§3.2.2 stage 1 — the closed kind set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkErrorKind {
    /// A selector survives into link (C-LINK-1).
    UnboundSlot,
    /// A pinned ref the bound view cannot satisfy, a definition-pinned profile the bound
    /// chain doesn't contain, or a chain `extends` target that is missing/cyclic.
    VersionConflict,
    /// A conditioned rule without a complete debt record (T-LCD-05).
    MissingDebtRecord,
    /// A `target_ref` naming no registered target.
    UnknownTarget,
    /// A bound profile carries no `ProfileTestReport` (AC-R-2.3.3-13;
    /// ADR-0125 d.1).
    ProfileUntested,
    /// A bound profile's `ProfileTestReport` has a failing validity section
    /// (AC-R-2.3.3-13).
    ProfileInvalid,
    /// A `DRIFT`/`UNSUPPORTED` conformance record on a capability in a bound
    /// rule's dependency set, without a recorded intent (ADR-0125 d.3).
    CapabilityDrift,
}

impl LinkErrorKind {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            LinkErrorKind::UnboundSlot => "unbound_slot",
            LinkErrorKind::VersionConflict => "version_conflict",
            LinkErrorKind::MissingDebtRecord => "missing_debt_record",
            LinkErrorKind::UnknownTarget => "unknown_target",
            LinkErrorKind::ProfileUntested => "profile_untested",
            LinkErrorKind::ProfileInvalid => "profile_invalid",
            LinkErrorKind::CapabilityDrift => "capability_drift",
        }
    }
}

/// The §3.2.10 failure modes — all errors, all typed.
#[derive(Debug)]
pub enum CompileError {
    /// The input is not a sealed definition (sealed markers absent / refs unpinned at the
    /// document level).
    NotSealed {
        /// What was observed instead.
        detail: String,
    },
    /// The canonical-form hash does not match the recorded `version_id` (or the bytes are
    /// not canonical).
    NonCanonicalInput {
        /// The mismatch detail.
        detail: String,
    },
    /// `validate_assembly` reported error-severity diagnostics (§3.2.2 stage 0 — the
    /// compiler calls it, never re-implements it).
    InvalidDefinition {
        /// The complete diagnostic set (never fail-fast upstream).
        diagnostics: Vec<AssemblyDiagnostic>,
    },
    /// `LinkError{…}` — the typed diagnostics that produced it are attached.
    LinkError {
        /// The closed kind.
        kind: LinkErrorKind,
        /// What failed.
        detail: String,
        /// The `C-*` diagnostics (e.g. the `C-LINK-1` set from `link_precheck`).
        diagnostics: Vec<AssemblyDiagnostic>,
    },
    /// No profile bound and no admissible `fallback_profile` (ADR-0124 §5).
    NoProfile {
        /// Why no binding was admissible.
        detail: String,
    },
    /// A profile-selector tie (`AmbiguousSelector` — resolution never guesses).
    AmbiguousSelector {
        /// The tied coordinates.
        detail: String,
    },
    /// `PlanError{unsupported_construct(node)}` — the expressiveness ceiling surfaces as
    /// an error (ADR-0019 D3).
    PlanError {
        /// The `hir_node_id` of the unsupported construct.
        node: String,
        /// What the construct is.
        detail: String,
    },
    /// `UnexpressibleSurface(entity, profile, reason)` — an error, never a warning.
    UnexpressibleSurface {
        /// The entity the surface cannot express.
        entity: String,
        /// The profile coordinate requiring it.
        profile: String,
        /// Why it is inexpressible.
        reason: String,
    },
    /// `UncheckableSurface` — a composite/synthesized surface without a plan-shaped
    /// mapping is inadmissible at C0 (ADR-0090 §6; ADR-0022 rule ii).
    UncheckableSurface {
        /// The surface name.
        surface: String,
        /// Why it is uncheckable.
        reason: String,
    },
    /// `DialectNarrowingUndeclared` — a surface narrows the schema dialect without the
    /// declaration the profile requires.
    DialectNarrowingUndeclared {
        /// The narrowing observed.
        detail: String,
    },
    /// `TargetError` — a stage-4 lowering/lifting failure (§3.2.10).
    TargetError {
        /// What failed.
        detail: String,
    },
    /// `ProtocolVersionMismatch` — the edge's protocol-version refusal (§3.2.10: "at the
    /// edge"). No protocol edge exists at C0 — declared so the §3.2.10 set is closed
    /// (CC8); a stage-3/4 emitter fills it.
    #[allow(dead_code)]
    ProtocolVersionMismatch {
        /// What failed.
        detail: String,
    },
    /// `SealError{non_canonical}` — the bundle does not re-encode to the hashed bytes.
    SealError {
        /// The mismatch detail.
        detail: String,
    },
    /// A `ModelProfile/1` input fails its schema (missing member, closed-enum miss, a
    /// profile chain deeper than the C0 two-level bound, a cycle).
    InvalidModelProfile {
        /// The violation.
        detail: String,
    },
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompileError::NotSealed { detail } => write!(f, "NotSealed: {detail}"),
            CompileError::NonCanonicalInput { detail } => {
                write!(f, "NonCanonicalInput: {detail}")
            }
            CompileError::InvalidDefinition { diagnostics } => write!(
                f,
                "InvalidDefinition[{}]: {}",
                diagnostics.len(),
                diagnostics
                    .iter()
                    .map(|d| d.code.code())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            CompileError::LinkError { kind, detail, .. } => {
                write!(f, "LinkError{{{}:{detail}}}", kind.name())
            }
            CompileError::NoProfile { detail } => write!(f, "NoProfile: {detail}"),
            CompileError::AmbiguousSelector { detail } => {
                write!(f, "AmbiguousSelector: {detail}")
            }
            CompileError::PlanError { node, detail } => {
                write!(f, "PlanError{{unsupported_construct({node}): {detail}}}")
            }
            CompileError::UnexpressibleSurface {
                entity,
                profile,
                reason,
            } => write!(f, "UnexpressibleSurface({entity}, {profile}): {reason}"),
            CompileError::UncheckableSurface { surface, reason } => {
                write!(f, "UncheckableSurface({surface}): {reason}")
            }
            CompileError::DialectNarrowingUndeclared { detail } => {
                write!(f, "DialectNarrowingUndeclared: {detail}")
            }
            CompileError::TargetError { detail } => write!(f, "TargetError: {detail}"),
            CompileError::ProtocolVersionMismatch { detail } => {
                write!(f, "ProtocolVersionMismatch: {detail}")
            }
            CompileError::SealError { detail } => write!(f, "SealError{{non_canonical}}: {detail}"),
            CompileError::InvalidModelProfile { detail } => {
                write!(f, "InvalidModelProfile: {detail}")
            }
        }
    }
}

impl std::error::Error for CompileError {}

/// `trace(bundle, locator)` failure — the locator grammar is closed; an unresolvable
/// locator is a typed error, never an empty answer (the map is total).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceError {
    /// The locator that did not resolve.
    pub locator: String,
}
