//! The §3.1.4 kernel validation error set (plus `MigrationLoss` and `DialectIncompatible` from
//! §3.1.10) as the closed [`HirError`] sum. Errors are **typed, never warnings** (§3.1.7);
//! ADR-0148 mirrors the set one-to-one as the `C-KERN-*` diagnostic codes.
//!
//! The one implementation-level addition is [`HirError::SchemaViolation`]: the spec's set
//! covers *semantic* violations of a well-formed document; an encoding that does not meet a
//! kind's fixed product type (a missing required member, a wrong-typed member, an unknown
//! member of a closed product) is reported here rather than being forced into a semantic code.
//! Recorded in the S1.4 build ADR.

use std::fmt;

/// The closed HIR/1 validation/operation error set (§3.1.4, §3.1.10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirError {
    /// A kind name not registered in the closed per-dialect sums (`EntityKind`, `EdgeKind`,
    /// the `EffectAttributes` sums, `ValidatorKind`, `AgentProcessKind`, …).
    UnknownKind {
        /// The unregistered kind spelling.
        kind: String,
    },
    /// `hir_version` / a kind schema's dialect is not one this build supports (`HIR/1`).
    DialectUnsupported {
        /// The unsupported dialect tag.
        dialect: String,
    },
    /// A `Ref` does not resolve to a node of an allowed kind in the document, or a selector /
    /// display name / mis-pinned `version_id` appears where a sealed form requires a pin.
    UnresolvedRef {
        /// What could not be resolved.
        detail: String,
    },
    /// A cycle in `depends-on`, `supersedes` or `delegated-to` inside a definition.
    CycleDetected {
        /// The cycle description.
        detail: String,
    },
    /// V-EFF: a `Procedure`'s derived effects are not covered by the in-scope `authorizes`
    /// grants (coverage by domain and scope).
    EffectUncovered {
        /// The uncovered effect / procedure.
        detail: String,
    },
    /// A diff whose `authority_delta` is `widening` from a non-`human` origin (or `evolution`
    /// always), or an `ext` slot that would decide authority/budget/validity.
    AuthorityWidening {
        /// What widened and where.
        detail: String,
    },
    /// Budget monotone containment violated: a child `Budget`, a `Goal.budget` or a
    /// `delegated-to` budget exceeds its parent dimension-wise.
    BudgetExceedsParent {
        /// The dimension and bounds.
        detail: String,
    },
    /// A `conditioned_on ≠ null` `HarnessRule` without a complete assumption-debt record
    /// (T-LCD-05), or a diff touching a conditioned rule without carrying its record.
    ConditionedRuleIncomplete {
        /// The rule's id.
        rule_id: String,
    },
    /// Input bytes are not the canonical encoding (duplicate keys, non-sorted members,
    /// insignificant whitespace, or a serialization that does not round-trip).
    NonCanonicalInput {
        /// Where the encoding departed from canonical form.
        detail: String,
    },
    /// An `Opaque` step or `CompiledPayload` without a declared interface (T-LCD-02 ceiling:
    /// opacity is legal only as the body of a declared interface).
    OpaqueWithoutInterface {
        /// The offending leaf/step.
        detail: String,
    },
    /// A string-shaped semantic construct outside the two leaves, a model-identity-typed
    /// field outside `ProfileRef`/`surface` (T-LCD-01), or a surface record on a kind that
    /// has none.
    UnexpressibleSurface {
        /// What could not be expressed.
        detail: String,
    },
    /// A kind with no registered home — the reflexive-LCD guard that nothing hides between
    /// planes (ADR-0012 D2; `hh_ontology::UnclassifiedKind` at the HIR layer).
    UnclassifiedKind {
        /// The kind that has no home.
        kind: String,
    },
    /// A node or edge without its `ProvenanceRecord`, or a `Text` leaf without provenance
    /// (DF-S1.3-1 — the record is mandatory on every node and edge).
    MissingProvenance {
        /// What was missing the record.
        what: String,
    },
    /// A stamped authority exceeding what `origin` + in-record evidence can confer
    /// (delegated from `ProvenanceRecord::validate`; §8.1 #5).
    AuthorityExceedsOrigin {
        /// The ceiling the origin/evidence confers.
        ceiling: String,
        /// The claimed class.
        claimed: String,
    },
    /// A `Text` leaf carrying `authority > external` whose origin cannot confer it (R-TEXT;
    /// §3.1.11 role-placement precondition).
    TextAboveExternal {
        /// The claimed authority.
        authority: String,
    },
    /// `taint ≠ ∅` with `authority > external` (delegated from `ProvenanceRecord::validate`).
    TaintedAboveExternal {
        /// The claimed authority.
        authority: String,
    },
    /// A `model_claim`-only evidence basis satisfying a `Validator` (a model claim can never
    /// satisfy a Validator alone), or an endorsement with no legitimate basis.
    IllegitimateEndorsement {
        /// What was illegitimately endorsed.
        detail: String,
    },
    /// A record scoped `definition` without the `seal`-conferred `definition` authority —
    /// the scope write ceiling (§8.1 #3; ADR-0033/0034).
    ScopeCeilingExceeded {
        /// The scope and authority.
        detail: String,
    },
    /// `migrate` could not carry the document losslessly — the loss is declared, never
    /// silent (§3.1.7; CC3).
    MigrationLoss {
        /// What would be lost.
        lost: Vec<String>,
    },
    /// A definition and a variant's `dialect_range` disagree (§3.1.10; also `migrate`'s
    /// unsupported from/to pair).
    DialectIncompatible {
        /// The incompatible pair.
        detail: String,
    },
    /// Implementation-level (see module doc): an encoding that does not meet a kind's fixed
    /// product type — a missing/unknown/mis-typed member — rather than a semantic violation.
    SchemaViolation {
        /// The member and what was wrong with it.
        detail: String,
    },
    /// A literal credential (a known mask-set value, a registered credential pattern, or a
    /// canary) appears in a definition's canonical bytes or in a `HirDiff`'s ops — the
    /// seal/evolution gate R-2.8.3's secret detectors run before sealing (§5g.3; S1.13).
    /// Raised by `hh_secrets::seal_checked`/`check_diff`, collected alongside the other
    /// `seal` failures.
    SecretValueInDefinition {
        /// Where the detector fired (a path or node ref + the detector name — never the
        /// matched bytes).
        detail: String,
    },
    /// V-E1-1 (§5d.1 §3): a `ToolCapability` declared `effects` as the empty
    /// set — `pure` is the only honest empty.
    EmptyEffectSet,
    /// V-E1-4 (§5d.1 §3): a `scope_bindings` row's `param_path` does not resolve
    /// in `input_schema` (the ADR-0052 `UnmappedParameter` spelling).
    UnmappedParameter {
        /// The unmapped path.
        param_path: String,
    },
    /// V-E1-3 (§5d.1 §3): a scope-bearing effect domain with no `scope_bindings`
    /// row and no `scope_bindings_unknown` (the ADR-0052 `UnscopedParameter`
    /// spelling).
    UnscopedParameter {
        /// The unscoped domain.
        domain: String,
    },
    /// V-E1-7 (§5d.1 §3; ADR-0034 P7): a lifted source (`mcp_listing` /
    /// `participant_supplied`) whose declarations are stamped above
    /// `unverified` — a `pin` endorsement raises the class by minting a new
    /// version, never by registering over it.
    LiftedDeclarationsNotUnverified {
        /// The lifted source kind.
        source_kind: String,
    },
    /// V-E1-8 (§5d.1 §3; ADR-0089 D1): a `cost_model.declared` estimate claimed
    /// `confidence = exact` with no `measured_ref` on the record.
    ExactCostUnmeasured {
        /// The dimension claimed exact.
        dimension: String,
    },
    /// V-E1-10 (§5d.1 §3): a `procedure`-sourced capability whose declared
    /// `effects` differ from the procedure's derived effects.
    DerivedEffectsMismatch {
        /// Which side drifted.
        detail: String,
    },
    /// I-DISCOVERY (§5d.3 §2; ADR-0093 D8): a definition admits `deferred`
    /// without binding a `discover_surfaces` capability (`exposure_hint.
    /// discovery = true`).
    NoDiscoverySurface,
    /// §5g.5 L4 (S1.23): a sealed form carrying an `assembly.extensions.refs[]`
    /// member that is not fully pinned — a surviving `locator.selector`, or a
    /// missing `locator.resolved`/`fetched_at`/`content` pin.
    UnpinnedInSealedForm {
        /// Which ref and which pin member is missing.
        detail: String,
    },
}

impl fmt::Display for HirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HirError::UnknownKind { kind } => write!(f, "UnknownKind: {kind}"),
            HirError::DialectUnsupported { dialect } => {
                write!(f, "DialectUnsupported: {dialect}")
            }
            HirError::UnresolvedRef { detail } => write!(f, "UnresolvedRef: {detail}"),
            HirError::CycleDetected { detail } => write!(f, "CycleDetected: {detail}"),
            HirError::EffectUncovered { detail } => write!(f, "EffectUncovered: {detail}"),
            HirError::AuthorityWidening { detail } => write!(f, "AuthorityWidening: {detail}"),
            HirError::BudgetExceedsParent { detail } => {
                write!(f, "BudgetExceedsParent: {detail}")
            }
            HirError::ConditionedRuleIncomplete { rule_id } => {
                write!(f, "ConditionedRuleIncomplete: {rule_id}")
            }
            HirError::NonCanonicalInput { detail } => write!(f, "NonCanonicalInput: {detail}"),
            HirError::OpaqueWithoutInterface { detail } => {
                write!(f, "OpaqueWithoutInterface: {detail}")
            }
            HirError::UnexpressibleSurface { detail } => {
                write!(f, "UnexpressibleSurface: {detail}")
            }
            HirError::UnclassifiedKind { kind } => write!(f, "UnclassifiedKind: {kind}"),
            HirError::MissingProvenance { what } => write!(f, "MissingProvenance: {what}"),
            HirError::AuthorityExceedsOrigin { ceiling, claimed } => write!(
                f,
                "AuthorityExceedsOrigin: claimed {claimed} above ceiling {ceiling}"
            ),
            HirError::TextAboveExternal { authority } => {
                write!(f, "TextAboveExternal: {authority}")
            }
            HirError::TaintedAboveExternal { authority } => {
                write!(f, "TaintedAboveExternal: {authority}")
            }
            HirError::IllegitimateEndorsement { detail } => {
                write!(f, "IllegitimateEndorsement: {detail}")
            }
            HirError::ScopeCeilingExceeded { detail } => {
                write!(f, "ScopeCeilingExceeded: {detail}")
            }
            HirError::MigrationLoss { lost } => {
                write!(f, "MigrationLoss: lost [{}]", lost.join(", "))
            }
            HirError::DialectIncompatible { detail } => {
                write!(f, "DialectIncompatible: {detail}")
            }
            HirError::SchemaViolation { detail } => write!(f, "SchemaViolation: {detail}"),
            HirError::SecretValueInDefinition { detail } => {
                write!(f, "SecretValueInDefinition: {detail}")
            }
            HirError::EmptyEffectSet => write!(f, "EmptyEffectSet: declare `pure`"),
            HirError::UnmappedParameter { param_path } => {
                write!(f, "UnmappedParameter: {param_path}")
            }
            HirError::UnscopedParameter { domain } => {
                write!(f, "UnscopedParameter: {domain}")
            }
            HirError::LiftedDeclarationsNotUnverified { source_kind } => {
                write!(f, "LiftedDeclarationsNotUnverified: {source_kind}")
            }
            HirError::ExactCostUnmeasured { dimension } => {
                write!(f, "ExactCostUnmeasured: {dimension}")
            }
            HirError::DerivedEffectsMismatch { detail } => {
                write!(f, "DerivedEffectsMismatch: {detail}")
            }
            HirError::NoDiscoverySurface => write!(f, "NoDiscoverySurface"),
            HirError::UnpinnedInSealedForm { detail } => {
                write!(f, "UnpinnedInSealedForm: {detail}")
            }
        }
    }
}

impl std::error::Error for HirError {}

/// The report `validate` produces on success (§3.1.7): which invariant checks ran. The error
/// path is the `[Error]` list — failures are collected, not fail-fast, so a document reports
/// every violation it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    /// The invariant checks that ran, by name (stable identifiers for the AC table).
    pub checks_run: Vec<&'static str>,
    /// How many nodes and edges were checked.
    pub node_count: usize,
    /// Edge count.
    pub edge_count: usize,
}
