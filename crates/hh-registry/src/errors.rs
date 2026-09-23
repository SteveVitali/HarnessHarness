//! The closed `RegistryError` sum (spec §6.2 §2 error columns + §5 failure modes).
//! Every operation failure is a typed variant — and a **record**: the store appends
//! a `RegistryDiagnostic` for each (R10; failures are never instrument start-up
//! failures). `ext` can never introduce a failure mode or a status vocabulary (R5).

use std::collections::BTreeMap;

use crate::kinds::ConformanceVerdict;

/// The failure modes of the `registry/1` operations (spec §6.2 §2 — the union of
/// the `register`, `publish`, `resolve`, `query`, `slot_choices`, `record_conformance`
/// and `deprecate`/`yank`/`revoke` columns this slice lands).
#[derive(Debug, Clone, PartialEq)]
pub enum RegistryError {
    /// An unlisted `kind` — growth is `registry/2`, never `ext` (R2; AC-2).
    UnknownRecordKind {
        /// The spelling that failed.
        kind: String,
    },
    /// The record body is not schema-valid (`{path}` names the offending member —
    /// including a status outside the three R5 vocabularies, a record kind without
    /// a landed `registry/1` schema, or an invalid registrar record — R9).
    SchemaViolation {
        /// The member path.
        path: String,
        /// What was wrong.
        detail: String,
    },
    /// A `variant`'s `class_ref` names no registered `ClassRecord`.
    UnknownClass {
        /// The class ref that failed to resolve.
        class_ref: String,
    },
    /// `contract_range ∩ class.contract_version = ∅`, or a `resolve` requirement
    /// (`class_id`/`contract_version`) the record does not satisfy.
    ContractIncompatible {
        /// What was incompatible.
        detail: String,
    },
    /// A conditioned rule without a complete `AssumptionDebtRecord` (T-LCD-05).
    ConditionedRuleIncomplete {
        /// The rule missing debt data.
        rule_id: String,
    },
    /// `implementation` not pinned by `ContentAddress` (R7 — a digest-less foreign
    /// locator is a claim, never a pin).
    UnpinnedImplementation,
    /// `registrar.authority ∉ {kernel, definition}` without a `trust_record_ref`
    /// (ADR-0153).
    TrustRecordRequired,
    /// `implementation.placement` inadmissible for the registrar's authority —
    /// third-party `in_process` is refused (ADR-0153 D1; CF-141/CF-378).
    LocalityInadmissible {
        /// The registrar origin tag.
        origin: String,
        /// The refused placement.
        placement: String,
    },
    /// The record's `dialect_range` excludes `registry/1`, or the envelope names a
    /// dialect this store does not write.
    DialectIncompatible {
        /// The dialect range that failed.
        dialect_range: String,
    },
    /// A label already used for this name (labels are unique per name).
    LabelReused {
        /// The name.
        name: String,
        /// The reused label.
        label: String,
    },
    /// `supersedes` names a version outside the name's line/kind.
    KindMismatch {
        /// What mismatched.
        detail: String,
    },
    /// A widening/loosening successor without a `principal`-authority attested
    /// provenance (§8.3 #6).
    AuthorityWideningRequiresHuman,
    /// A publish into a namespace the registrar may not write (`hh/` is
    /// kernel-owned; a non-owner publish is forbidden — ADR-0153 D2).
    NamespaceForbidden {
        /// The namespace.
        namespace: String,
    },
    /// A second `publish` of `(namespace, name)` by another publisher or across
    /// kinds (AC-8; `(namespace, name)` is unique across all kinds).
    NameCollision {
        /// The colliding name.
        namespace: String,
        /// The colliding name.
        name: String,
    },
    /// `publish` under `require_conformance` the record does not satisfy
    /// (`publisher_claim` never satisfies `probed`).
    ConformanceRequired {
        /// The floor that failed.
        floor: String,
    },
    /// A `probed` floor whose only reports are stale (`stale` reports satisfy no
    /// floor — ADR-0152 D3).
    SuiteStale,
    /// Nothing satisfies the selector (+ requirements) — resolve.
    Unresolved {
        /// What was sought.
        detail: String,
    },
    /// The selector matches more than one head.
    Ambiguous {
        /// The candidate `version_id`s.
        candidates: Vec<String>,
    },
    /// `resolve(mode = execute)` of a revoked version (revocation is a
    /// `RevocationRecord`, never a hidden delete).
    Revoked {
        /// The revoked `version_id`.
        version_id: String,
        /// The revocation reason.
        reason: String,
    },
    /// The resolved version (or a dependant) is stale — `depends_on_revoked[]`
    /// annotated, never hidden (S4/CC3), or a name binding past `policy.max_age`.
    Stale {
        /// The stale-by-dependency members.
        depends_on_revoked: Vec<String>,
        /// The stale reason when it is a freshness expiry.
        detail: String,
    },
    /// `resolve(mode = execute)` under a conformance floor the record does not
    /// meet — the derived vector is attached (ADR-0152 D6).
    ConformanceBelowFloor {
        /// The derived `variant_conformance_vector`.
        vector: BTreeMap<String, ConformanceVerdict>,
    },
    /// `query`/`slot_choices` over an unregistered `registry_snapshot_id`.
    UnknownSnapshot {
        /// The snapshot id.
        snapshot_id: String,
    },
    /// A `query` clause over a field outside the declared queryable set (the
    /// predicate is closed and quantifier-free — never relevance over free text).
    UnknownField {
        /// The field spelling.
        field: String,
    },
    /// `slot_choices`/`resolve` under `conformance_floor ≠ none` for a class with
    /// no `conformance_suite_ref` (ADR-0152 — its variants resolve only under
    /// `require_conformance = none`).
    SuiteMissing {
        /// The class id.
        class_id: String,
    },
    /// `revoke`/`deprecate`/`yank` by an authority the namespace policy does not
    /// admit (`who_may_*`; another publisher's version needs ≥ `principal`).
    AuthorityInsufficient {
        /// The operation.
        operation: String,
    },
    /// A `supersedes`/revocation edge that would close the version DAG.
    CycleDetected {
        /// The newer version.
        newer: String,
        /// The older version.
        older: String,
    },
    /// A non-ancestor/sibling edge (allowed only with `reason = fork`).
    NotAncestorOrSibling,
    /// An operation on a record the store does not hold.
    UnknownVersion {
        /// The `version_id`.
        version_id: String,
    },
}

impl RegistryError {
    /// The stable reason spelling recorded on the `RegistryDiagnostic` (R10).
    pub fn reason(&self) -> &'static str {
        match self {
            RegistryError::UnknownRecordKind { .. } => "UnknownRecordKind",
            RegistryError::SchemaViolation { .. } => "SchemaViolation",
            RegistryError::UnknownClass { .. } => "UnknownClass",
            RegistryError::ContractIncompatible { .. } => "ContractIncompatible",
            RegistryError::ConditionedRuleIncomplete { .. } => "ConditionedRuleIncomplete",
            RegistryError::UnpinnedImplementation => "UnpinnedImplementation",
            RegistryError::TrustRecordRequired => "TrustRecordRequired",
            RegistryError::LocalityInadmissible { .. } => "LocalityInadmissible",
            RegistryError::DialectIncompatible { .. } => "DialectIncompatible",
            RegistryError::LabelReused { .. } => "LabelReused",
            RegistryError::KindMismatch { .. } => "KindMismatch",
            RegistryError::AuthorityWideningRequiresHuman => "AuthorityWideningRequiresHuman",
            RegistryError::NamespaceForbidden { .. } => "NamespaceForbidden",
            RegistryError::NameCollision { .. } => "NameCollision",
            RegistryError::ConformanceRequired { .. } => "ConformanceRequired",
            RegistryError::SuiteStale => "SuiteStale",
            RegistryError::Unresolved { .. } => "Unresolved",
            RegistryError::Ambiguous { .. } => "Ambiguous",
            RegistryError::Revoked { .. } => "Revoked",
            RegistryError::Stale { .. } => "Stale",
            RegistryError::ConformanceBelowFloor { .. } => "ConformanceBelowFloor",
            RegistryError::UnknownSnapshot { .. } => "UnknownSnapshot",
            RegistryError::UnknownField { .. } => "UnknownField",
            RegistryError::SuiteMissing { .. } => "SuiteMissing",
            RegistryError::AuthorityInsufficient { .. } => "AuthorityInsufficient",
            RegistryError::CycleDetected { .. } => "CycleDetected",
            RegistryError::NotAncestorOrSibling => "NotAncestorOrSibling",
            RegistryError::UnknownVersion { .. } => "UnknownVersion",
        }
    }
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.reason())?;
        match self {
            RegistryError::UnknownRecordKind { kind } => write!(f, ": {kind}"),
            RegistryError::SchemaViolation { path, detail } => {
                write!(f, ": {path}: {detail}")
            }
            RegistryError::UnknownClass { class_ref } => write!(f, ": {class_ref}"),
            RegistryError::ContractIncompatible { detail } => write!(f, ": {detail}"),
            RegistryError::ConditionedRuleIncomplete { rule_id } => write!(f, ": {rule_id}"),
            RegistryError::LocalityInadmissible { origin, placement } => {
                write!(f, ": {origin} at {placement}")
            }
            RegistryError::DialectIncompatible { dialect_range } => {
                write!(f, ": {dialect_range}")
            }
            RegistryError::NamespaceForbidden { namespace } => write!(f, ": {namespace}"),
            RegistryError::NameCollision { namespace, name } => {
                write!(f, ": {namespace}/{name}")
            }
            RegistryError::ConformanceRequired { floor } => write!(f, ": {floor}"),
            RegistryError::Unresolved { detail } => write!(f, ": {detail}"),
            RegistryError::Ambiguous { candidates } => write!(f, ": {candidates:?}"),
            RegistryError::Revoked { version_id, reason } => {
                write!(f, ": {version_id} ({reason})")
            }
            RegistryError::Stale {
                depends_on_revoked,
                detail,
            } => {
                write!(f, ": {detail} {depends_on_revoked:?}")
            }
            RegistryError::ConformanceBelowFloor { vector } => write!(f, ": {vector:?}"),
            RegistryError::UnknownSnapshot { snapshot_id } => write!(f, ": {snapshot_id}"),
            RegistryError::UnknownField { field } => write!(f, ": {field}"),
            RegistryError::SuiteMissing { class_id } => write!(f, ": {class_id}"),
            RegistryError::AuthorityInsufficient { operation } => write!(f, ": {operation}"),
            RegistryError::CycleDetected { newer, older } => {
                write!(f, ": {newer} -> {older}")
            }
            RegistryError::UnknownVersion { version_id } => write!(f, ": {version_id}"),
            _ => Ok(()),
        }
    }
}

impl std::error::Error for RegistryError {}
