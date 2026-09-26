//! The closed `registry/1` vocabulary (spec §6.2 §3; ADR-0151 D2/D4, R5): the
//! `RecordKind` list — growth is a `registry/2` dialect bump, never `ext` — and the
//! status sums of the three orthogonal axes (*admission* per record, *publication*
//! per name-history entry, *conformance* per declaration field). A fourth status
//! vocabulary anywhere in the registry is a schema error (R5, CF-324). The
//! publication vocabulary itself is **reused** from `hh-identity`
//! ([`hh_identity::names::NameStatus`]) — one scheme per concern (CC1).

use hh_identity::kinds::RecordKind as IdentityKind;

/// The closed `registry/1` record-kind list (ADR-0151 D2 as amended — CF-387 adds
/// `participant` and `adapter`; `conformance_report.subject_kind` gains
/// `snapshot_pair` per the Phase-4 amendment). `UnknownRecordKind` is an error;
/// adding a *component class* is a `ClassRecord` registration, never a new kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RecordKind {
    /// A component class (`ClassRecord`).
    Class,
    /// A component variant (`VariantRecord`).
    Variant,
    /// A conformance suite pinned to a contract version.
    ConformanceSuite,
    /// An immutable conformance report.
    ConformanceReport,
    /// A `NamespaceRecord`.
    Namespace,
    /// A name-history entry (produced by `publish`, never registered directly).
    NameHistoryEntry,
    /// A `registry_snapshot_id` record.
    RegistrySnapshot,
    /// A foreign-registry import (quarantined by default).
    ForeignImport,
    /// A sealed Harness Definition.
    SealedDefinition,
    /// A `ToolCapability` capability record.
    Capability,
    /// An `ExtensionRecord`.
    Extension,
    /// A `ModelProfile/1` version.
    ModelProfile,
    /// A profile test report.
    ProfileTestReport,
    /// A wire dialect.
    WireDialect,
    /// A model role table.
    ModelRoleTable,
    /// A procedure profile.
    ProcedureProfile,
    /// A surface family.
    SurfaceFamily,
    /// A `MetricDeclaration`.
    MetricDeclaration,
    /// A `Validator` / oracle declaration.
    Validator,
    /// An environment record.
    EnvironmentRecord,
    /// An environment family.
    EnvironmentFamily,
    /// A `TrustRootPolicy`.
    TrustRootPolicy,
    /// An identity profile (`idp/N`).
    IdentityProfile,
    /// A hosted participant descriptor (never a variant — T-LCD-06).
    Participant,
    /// An `AdapterRecord`.
    Adapter,
}

impl RecordKind {
    /// Every kind, in declaration order.
    pub const ALL: [RecordKind; 25] = [
        RecordKind::Class,
        RecordKind::Variant,
        RecordKind::ConformanceSuite,
        RecordKind::ConformanceReport,
        RecordKind::Namespace,
        RecordKind::NameHistoryEntry,
        RecordKind::RegistrySnapshot,
        RecordKind::ForeignImport,
        RecordKind::SealedDefinition,
        RecordKind::Capability,
        RecordKind::Extension,
        RecordKind::ModelProfile,
        RecordKind::ProfileTestReport,
        RecordKind::WireDialect,
        RecordKind::ModelRoleTable,
        RecordKind::ProcedureProfile,
        RecordKind::SurfaceFamily,
        RecordKind::MetricDeclaration,
        RecordKind::Validator,
        RecordKind::EnvironmentRecord,
        RecordKind::EnvironmentFamily,
        RecordKind::TrustRootPolicy,
        RecordKind::IdentityProfile,
        RecordKind::Participant,
        RecordKind::Adapter,
    ];

    /// The canonical `registry/1` spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RecordKind::Class => "class",
            RecordKind::Variant => "variant",
            RecordKind::ConformanceSuite => "conformance_suite",
            RecordKind::ConformanceReport => "conformance_report",
            RecordKind::Namespace => "namespace",
            RecordKind::NameHistoryEntry => "name_history_entry",
            RecordKind::RegistrySnapshot => "registry_snapshot",
            RecordKind::ForeignImport => "foreign_import",
            RecordKind::SealedDefinition => "sealed_definition",
            RecordKind::Capability => "capability",
            RecordKind::Extension => "extension",
            RecordKind::ModelProfile => "model_profile",
            RecordKind::ProfileTestReport => "profile_test_report",
            RecordKind::WireDialect => "wire_dialect",
            RecordKind::ModelRoleTable => "model_role_table",
            RecordKind::ProcedureProfile => "procedure_profile",
            RecordKind::SurfaceFamily => "surface_family",
            RecordKind::MetricDeclaration => "metric_declaration",
            RecordKind::Validator => "validator",
            RecordKind::EnvironmentRecord => "environment_record",
            RecordKind::EnvironmentFamily => "environment_family",
            RecordKind::TrustRootPolicy => "trust_root_policy",
            RecordKind::IdentityProfile => "identity_profile",
            RecordKind::Participant => "participant",
            RecordKind::Adapter => "adapter",
        }
    }

    /// Parse the canonical spelling. Unlisted kinds are `UnknownRecordKind` (R2).
    pub fn parse(s: &str) -> Option<RecordKind> {
        RecordKind::ALL.iter().copied().find(|k| k.as_str() == s)
    }

    /// Whether this kind has a Stage-1 record schema in this store. The kind list is
    /// closed at 25; the record *shapes* land with their owning stage — a parseable
    /// kind without a landed schema fails `register`/`from_json` with
    /// `SchemaViolation{path: "kind"}` (never silently admitted — R2/CC3).
    pub fn has_stage1_schema(self) -> bool {
        matches!(
            self,
            RecordKind::Class
                | RecordKind::Variant
                | RecordKind::ConformanceSuite
                | RecordKind::ConformanceReport
                | RecordKind::Namespace
                | RecordKind::RegistrySnapshot
                | RecordKind::ForeignImport
                | RecordKind::Capability
                | RecordKind::MetricDeclaration
                | RecordKind::Validator
        )
    }

    /// The `idp/1` domain tag under which this kind's `version_id` is minted (N3 —
    /// per-kind domain separation). Where `hh-identity`'s closed `RecordKind`
    /// already dedicates a domain tag to this record family the store **reuses it**
    /// (CC1 — one identity scheme; the same `VariantRecord` carries one `version_id`
    /// wherever it is referenced); the remaining registry kinds take the additive
    /// `registry.<kind>` tag (ADR-0239).
    pub fn domain_tag(self) -> String {
        match self {
            RecordKind::Variant => IdentityKind::VariantRecord.domain_tag().to_string(),
            RecordKind::SealedDefinition => IdentityKind::SealedDefinition.domain_tag().to_string(),
            RecordKind::Capability => IdentityKind::CapabilityDeclaration.domain_tag().to_string(),
            RecordKind::Extension => IdentityKind::ExtensionRecord.domain_tag().to_string(),
            RecordKind::ModelProfile => IdentityKind::ModelProfile.domain_tag().to_string(),
            RecordKind::Validator => IdentityKind::Validator.domain_tag().to_string(),
            RecordKind::IdentityProfile => IdentityKind::IdentityProfile.domain_tag().to_string(),
            other => format!("registry.{}", other.as_str()),
        }
    }

    /// Whether this kind carries a declared **semantic projection** (N6). Variants
    /// do (the comparison coordinate survives a rename — AC-7); conformance reports
    /// and snapshots are version-only views over what they name.
    pub fn has_semantic_projection(self) -> bool {
        matches!(
            self,
            RecordKind::Variant
                | RecordKind::Class
                | RecordKind::SealedDefinition
                | RecordKind::Capability
        )
    }
}

/// The *admission* axis — per record (R5; ADR-0063's four values). `Revoked` is a
/// **derived** value (the lineage's `RevocationRecord`s), never authored onto an
/// envelope — the stored axis holds the base admission and `revoked` is computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// Resolved — first-party, checks passed.
    Resolved,
    /// Sealed — pinned inside a sealed closure.
    Sealed,
    /// Quarantined — pending trust review (foreign imports, signature-required kinds).
    Quarantined,
    /// Revoked — derived from a `RevocationRecord`; never assigned at `register`.
    Revoked,
}

impl Admission {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Admission::Resolved => "resolved",
            Admission::Sealed => "sealed",
            Admission::Quarantined => "quarantined",
            Admission::Revoked => "revoked",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<Admission> {
        Some(match s {
            "resolved" => Admission::Resolved,
            "sealed" => Admission::Sealed,
            "quarantined" => Admission::Quarantined,
            "revoked" => Admission::Revoked,
            _ => return None,
        })
    }
}

/// The four conformance **test kinds** (ADR-0152 D2 — the form; classes fill the
/// content from their ACs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestKind {
    /// Record schema, `declaration_schema` conformance, complete debt records,
    /// `required_inputs`.
    Static,
    /// Operations over canonical documents out-of-process; every closed-sum input.
    Contract,
    /// Fixtures with deterministic oracles (the only oracle class at C0).
    Executable,
    /// Kernel-relied invariants (never widens authority, declared vocabulary only).
    Property,
}

impl TestKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            TestKind::Static => "static",
            TestKind::Contract => "contract",
            TestKind::Executable => "executable",
            TestKind::Property => "property",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<TestKind> {
        Some(match s {
            "static" => TestKind::Static,
            "contract" => TestKind::Contract,
            "executable" => TestKind::Executable,
            "property" => TestKind::Property,
            _ => return None,
        })
    }
}

/// The suite test's declared driver (ADR-0152 D1). Out-of-process drivers are the
/// variant host at Stage 2; at Stage 1 they run in the reference runtime's test
/// harness (ADR-0152 (d)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestDriver {
    /// In-process.
    InProcess,
    /// Out-of-process (the `registry_ci` driver seam).
    OutOfProcess,
}

impl TestDriver {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            TestDriver::InProcess => "in-process",
            TestDriver::OutOfProcess => "out-of-process",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<TestDriver> {
        Some(match s {
            "in-process" => TestDriver::InProcess,
            "out-of-process" => TestDriver::OutOfProcess,
            _ => return None,
        })
    }
}

/// The oracle class of a suite test. **Deterministic-only at C0** (ADR-0047;
/// ADR-0152 D1) — judged oracles are C1 and inherit judge governance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OracleClass {
    /// A deterministic oracle.
    Deterministic,
}

impl OracleClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            OracleClass::Deterministic => "deterministic",
        }
    }

    /// Parse the canonical spelling; non-deterministic oracle classes are C1.
    pub fn parse(s: &str) -> Option<OracleClass> {
        match s {
            "deterministic" => Some(OracleClass::Deterministic),
            _ => None,
        }
    }
}

/// The one verdict vocabulary — Ontology v1 *observed conformance* (ADR-0152 D4).
/// `results[].verdict` and the per-field `probed` value share it; `unknown` is
/// never coerced (T-LCD-07) and `DRIFT` is data, never a silent exclusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConformanceVerdict {
    /// Observed support.
    Supported,
    /// Observed non-support.
    Unsupported,
    /// Partial support.
    Partial,
    /// Not applicable (with reason at the projection edge).
    NotApplicable,
    /// Never probed / not determinable — **never coerced** (T-LCD-07).
    Unknown,
    /// Declared but not executed this run.
    Skipped,
    /// Two admissible observations disagree — a stratification coordinate.
    Drift,
}

impl ConformanceVerdict {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ConformanceVerdict::Supported => "SUPPORTED",
            ConformanceVerdict::Unsupported => "UNSUPPORTED",
            ConformanceVerdict::Partial => "PARTIAL",
            ConformanceVerdict::NotApplicable => "NOT_APPLICABLE",
            ConformanceVerdict::Unknown => "UNKNOWN",
            ConformanceVerdict::Skipped => "SKIPPED",
            ConformanceVerdict::Drift => "DRIFT",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<ConformanceVerdict> {
        Some(match s {
            "SUPPORTED" => ConformanceVerdict::Supported,
            "UNSUPPORTED" => ConformanceVerdict::Unsupported,
            "PARTIAL" => ConformanceVerdict::Partial,
            "NOT_APPLICABLE" => ConformanceVerdict::NotApplicable,
            "UNKNOWN" => ConformanceVerdict::Unknown,
            "SKIPPED" => ConformanceVerdict::Skipped,
            "DRIFT" => ConformanceVerdict::Drift,
            _ => return None,
        })
    }
}

/// Who produced a conformance report (ADR-0152 D3). `publisher_claim` reports are
/// recorded and shown at `review` and **never count toward `probed`**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProducedBy {
    /// The registry CI driver (the variant host at Stage 2; the reference
    /// runtime's test harness at Stage 1).
    RegistryCi,
    /// A Lab run (`charged_to = instrument`).
    Lab,
    /// The publisher's own claim — inert until probed.
    PublisherClaim,
}

impl ProducedBy {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ProducedBy::RegistryCi => "registry_ci",
            ProducedBy::Lab => "lab",
            ProducedBy::PublisherClaim => "publisher_claim",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<ProducedBy> {
        Some(match s {
            "registry_ci" => ProducedBy::RegistryCi,
            "lab" => ProducedBy::Lab,
            "publisher_claim" => ProducedBy::PublisherClaim,
            _ => return None,
        })
    }
}

/// `implementation.placement` (ADR-0151 D5 as amended; CF-378). `locality` is the
/// derived view (`in-process` iff `placement = in_process`); `component_model` is
/// **reserved** — declared in the vocabulary, not admissible for a variant at this
/// stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Placement {
    /// In the kernel process.
    InProcess,
    /// A confined subprocess.
    SubprocessConfined,
    /// A container.
    Container,
    /// A remote host.
    Remote,
    /// Reserved (`component_model`) — vocabulary-held, inadmissible at this stage.
    ComponentModel,
}

impl Placement {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Placement::InProcess => "in_process",
            Placement::SubprocessConfined => "subprocess_confined",
            Placement::Container => "container",
            Placement::Remote => "remote",
            Placement::ComponentModel => "component_model",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<Placement> {
        Some(match s {
            "in_process" => Placement::InProcess,
            "subprocess_confined" => Placement::SubprocessConfined,
            "container" => Placement::Container,
            "remote" => Placement::Remote,
            "component_model" => Placement::ComponentModel,
            _ => return None,
        })
    }
}

/// `ClassRecord.cardinality` (ADR-0023).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    /// Exactly one variant bound.
    ExactlyOne,
    /// Zero or one.
    Optional,
    /// An ordered list (per-element `enabled` is the ablation switch).
    OrderedMany,
}

impl Cardinality {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Cardinality::ExactlyOne => "exactly-one",
            Cardinality::Optional => "optional",
            Cardinality::OrderedMany => "ordered-many",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<Cardinality> {
        Some(match s {
            "exactly-one" => Cardinality::ExactlyOne,
            "optional" => Cardinality::Optional,
            "ordered-many" => Cardinality::OrderedMany,
            _ => return None,
        })
    }
}

/// `conformance_report.subject_kind` (ADR-0152 D3–D4 as amended — `snapshot_pair`
/// is ADR-0203's `CompatibilityRecord`, CF-460).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectKind {
    /// A component variant.
    Variant,
    /// A model profile.
    Profile,
    /// A hosted participant.
    Participant,
    /// A snapshot pair (`CompatibilityRecord`).
    SnapshotPair,
}

impl SubjectKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SubjectKind::Variant => "variant",
            SubjectKind::Profile => "profile",
            SubjectKind::Participant => "participant",
            SubjectKind::SnapshotPair => "snapshot_pair",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<SubjectKind> {
        Some(match s {
            "variant" => SubjectKind::Variant,
            "profile" => SubjectKind::Profile,
            "participant" => SubjectKind::Participant,
            "snapshot_pair" => SubjectKind::SnapshotPair,
            _ => return None,
        })
    }
}

/// `RegistryPolicy.require_conformance` — the floor checked at `publish` and at
/// `resolve(mode = execute)` (ADR-0152 D6). Default `declared` at C0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequireConformance {
    /// No floor.
    None,
    /// The declaration must be present and complete (the Stage-1 default).
    Declared,
    /// Admissible non-stale reports must cover the declaration (`probed` strict
    /// mode — a `publisher_claim` never satisfies it).
    Probed,
}

impl RequireConformance {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RequireConformance::None => "none",
            RequireConformance::Declared => "declared",
            RequireConformance::Probed => "probed",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<RequireConformance> {
        Some(match s {
            "none" => RequireConformance::None,
            "declared" => RequireConformance::Declared,
            "probed" => RequireConformance::Probed,
            _ => return None,
        })
    }
}

/// A `NamespaceRecord.owners[]` member — a signer (C1 trust machinery) or a
/// principal (the Stage-1 ownership form).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerRef {
    /// A `TrustRootPolicy.accepted_signers` member (C1).
    Signer(String),
    /// A principal identity coordinate.
    Principal(String),
}

/// `NamespacePolicy.who_may_*` — the Stage-1 publish/deprecate/yank/revoke rule
/// (the full signer-anchored ownership surface is C1/Stage 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishRule {
    /// Only a `kernel`-authority registrar (`hh/`).
    KernelOnly,
    /// A namespace owner or a `principal`-authority registrar (`local/`).
    OwnersOrPrincipal,
}

impl PublishRule {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            PublishRule::KernelOnly => "kernel_only",
            PublishRule::OwnersOrPrincipal => "owners_or_principal",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<PublishRule> {
        Some(match s {
            "kernel_only" => PublishRule::KernelOnly,
            "owners_or_principal" => PublishRule::OwnersOrPrincipal,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kind_list_is_closed_at_twenty_five() {
        assert_eq!(RecordKind::ALL.len(), 25);
        let mut seen = std::collections::BTreeSet::new();
        for k in RecordKind::ALL {
            assert!(seen.insert(k.as_str()), "duplicate kind spelling {k:?}");
            assert_eq!(RecordKind::parse(k.as_str()), Some(k));
        }
        assert_eq!(RecordKind::parse("whatever"), None);
        assert_eq!(RecordKind::parse(""), None);
    }

    #[test]
    fn the_three_axes_vocabularies_are_closed() {
        assert_eq!(Admission::parse("provisional"), None);
        assert_eq!(ConformanceVerdict::parse("pass"), None);
        assert_eq!(ProducedBy::parse("ci"), None);
        for v in [
            Admission::Resolved,
            Admission::Sealed,
            Admission::Quarantined,
            Admission::Revoked,
        ] {
            assert_eq!(Admission::parse(v.as_str()), Some(v));
        }
        for v in [
            ConformanceVerdict::Supported,
            ConformanceVerdict::Unsupported,
            ConformanceVerdict::Partial,
            ConformanceVerdict::NotApplicable,
            ConformanceVerdict::Unknown,
            ConformanceVerdict::Skipped,
            ConformanceVerdict::Drift,
        ] {
            assert_eq!(ConformanceVerdict::parse(v.as_str()), Some(v));
        }
    }

    #[test]
    fn dedicated_kinds_reuse_the_identity_domain_tags() {
        // CC1: a VariantRecord's version id is minted under the one "variant"
        // domain — never a second registry-specific scheme for the same record.
        assert_eq!(RecordKind::Variant.domain_tag(), "variant");
        assert_eq!(RecordKind::SealedDefinition.domain_tag(), "hir.definition");
        assert_eq!(RecordKind::Class.domain_tag(), "registry.class");
        assert_eq!(
            RecordKind::ConformanceSuite.domain_tag(),
            "registry.conformance_suite"
        );
        // N3: every kind's tag is distinct and contains no framing separator.
        let mut tags = std::collections::BTreeSet::new();
        for k in RecordKind::ALL {
            assert!(tags.insert(k.domain_tag()));
            assert!(!k.domain_tag().contains('\u{1f}'));
        }
    }

    #[test]
    fn reserved_component_model_is_vocabulary_only() {
        assert_eq!(
            Placement::parse("component_model"),
            Some(Placement::ComponentModel)
        );
        assert_eq!(Placement::parse("in-process"), None); // derived, never spelled
    }
}
