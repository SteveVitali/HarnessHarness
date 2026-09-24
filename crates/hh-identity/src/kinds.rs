//! The **five identity kinds** and the `RecordKind` domain-tag table (spec §8.3 #3; ADR-0036
//! D1/D3; ADR-0048 §A.2).
//!
//! The five identity kinds fix *which* kind of id applies to each kind of thing (N3 domain
//! separation — a blob, a `Text` leaf and a node with equal bytes get distinct ids):
//!
//! - **content address** — raw bytes and trees ([`crate::ContentAddress`]);
//! - **version id** — every authored typed record (the [`RecordKind`] variants below);
//! - **semantic id** — HIR kinds only (the comparison coordinate; surface/provenance/ext/version
//!   excluded — N6);
//! - **allocated id** — runtime observations (`run_id ⊃ turn_id ⊃ model_call_id ⊃ tool_call_id`,
//!   `event_id`); allocated by the ledger, out of this crate's Stage-1 slice ([`AllocatedIdKind`]
//!   names them so the taxonomy is complete);
//! - **logical name** — `(namespace, name)` with an append-only history ([`crate::names`]).
//!
//! The invariants N1–N8 are encoded where they bite: N1 full digests only (no stored
//! abbreviation — [`crate::idp`] renders `<algorithm>:<hex>`), N2 self-describing, N3 domain
//! separation (this table), N4 equality by digest never by name/path, N5 sealed/executed/published
//! forms contain only pinned ids ([`RecordKind::is_sealed_form`]), N6 semantic vs version
//! coordinate ([`RecordKind::has_semantic_projection`]), N7 provider ids are claims, N8 one tree
//! rule + foreign digests as claims.

/// A typed record kind that receives a **version id** (§8.3 #3). Closed here for the Stage-1
/// slice; new record kinds are added **additively** by their owning ticket (CC8 — an additive
/// enum extension is an HIR-dialect bump with migration, never a silent second id scheme). Each
/// kind carries a distinct `domain_tag` so N3 domain separation holds by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordKind {
    /// An HIR node (has a semantic projection).
    HirNode,
    /// An HIR edge (has a semantic projection — `{kind, from, to, fields}`; S1.4).
    HirEdge,
    /// An HIR `Text` leaf (distinct domain from a blob with equal bytes — AC-2).
    HirTextLeaf,
    /// A sealed Harness Definition (pinned form — N5; has a semantic projection).
    SealedDefinition,
    /// A component `VariantRecord`.
    VariantRecord,
    /// A Model Profile version.
    ModelProfile,
    /// A permission policy.
    PermissionPolicy,
    /// A budget spec.
    Budget,
    /// A Validator / grader.
    Validator,
    /// A task-data manifest.
    TaskDataManifest,
    /// A capability declaration.
    CapabilityDeclaration,
    /// A run manifest (pinned form — N5).
    RunManifest,
    /// A bundle manifest (pinned form — N5).
    BundleManifest,
    /// A name-binding record (surface — no semantic id).
    NameBindingRecord,
    /// An identity profile (`idp/N`) — a `registry/1` record kind, bootstrapped.
    IdentityProfile,
    /// An extension record.
    ExtensionRecord,
    /// A registry record (pinned form — N5).
    RegistryRecord,
    /// A revocation record — a new version of the same kind with an empty body, so "revoked" is
    /// itself hash-chained (§8.3 #3).
    RevocationRecord,
    /// A configuration's **semantic** coordinate (seed excluded — the `configuration_id` body).
    Configuration,
    /// A configuration's **exact-bytes** coordinate (seed included — the
    /// `configuration_version_id` body).
    ConfigurationVersion,
    /// A `ContainmentPolicy/1` record (§5g.4) — the typed, provenance-bearing,
    /// content-addressed sandbox floor attached to the environment handle; has a
    /// semantic projection (`policy_id` — normalised policy members only;
    /// provenance, ext and ids excluded — ADR-0060 D1).
    ContainmentPolicy,
}

impl RecordKind {
    /// The domain tag hashed into every id of this kind (§8.3 #2 `domain_tags: map<RecordKind,
    /// bytes>`). Distinct per kind, so two kinds never collide on equal canonical bytes (N3).
    /// The tag contains no `\x1f` unit separator, which is what makes the framed digest
    /// unambiguous (see [`crate::idp::idp_digest`]).
    pub fn domain_tag(self) -> &'static str {
        match self {
            RecordKind::HirNode => "hir.node",
            RecordKind::HirEdge => "hir.edge",
            RecordKind::HirTextLeaf => "hir.text",
            RecordKind::SealedDefinition => "hir.definition",
            RecordKind::VariantRecord => "variant",
            RecordKind::ModelProfile => "model_profile",
            RecordKind::PermissionPolicy => "permission_policy",
            RecordKind::Budget => "budget",
            RecordKind::Validator => "validator",
            RecordKind::TaskDataManifest => "task_data_manifest",
            RecordKind::CapabilityDeclaration => "capability_declaration",
            RecordKind::RunManifest => "run_manifest",
            RecordKind::BundleManifest => "bundle_manifest",
            RecordKind::NameBindingRecord => "name_binding",
            RecordKind::IdentityProfile => "identity_profile",
            RecordKind::ExtensionRecord => "extension",
            RecordKind::RegistryRecord => "registry",
            RecordKind::RevocationRecord => "revocation",
            RecordKind::Configuration => "configuration",
            RecordKind::ConfigurationVersion => "configuration_version",
            RecordKind::ContainmentPolicy => "containment_policy",
        }
    }

    /// Whether this kind has a declared **semantic projection** (§8.3 #2: "semantic_id only for
    /// kinds with a declared semantic projection (HIR nodes/definitions — surface, provenance,
    /// ext, version excluded)"; N6). Name-binding, registry, revocation, run/bundle manifests and
    /// identity profiles are version-only.
    pub fn has_semantic_projection(self) -> bool {
        matches!(
            self,
            RecordKind::HirNode
                | RecordKind::HirEdge
                | RecordKind::HirTextLeaf
                | RecordKind::SealedDefinition
                | RecordKind::VariantRecord
                | RecordKind::ModelProfile
                | RecordKind::CapabilityDeclaration
                | RecordKind::Budget
                | RecordKind::Configuration
                | RecordKind::ContainmentPolicy
        )
    }

    /// Whether this kind is a **sealed / executed / published form** whose references must all be
    /// pinned ids — a selector or display name inside it is refused at `identify` with
    /// `UnresolvedRef` (N5; CC3 anchors here). §8.3 #7 names sealed definition, run manifest,
    /// bundle manifest and registry record.
    pub fn is_sealed_form(self) -> bool {
        matches!(
            self,
            RecordKind::SealedDefinition
                | RecordKind::RunManifest
                | RecordKind::BundleManifest
                | RecordKind::RegistryRecord
        )
    }
}

/// The **allocated id** family (runtime observations). Named here so the five-kind taxonomy is
/// complete; the ids themselves are minted by the run ledger (R-2.2.1, S1.5), not this crate.
/// N4: equality is by the allocated id, never by name or path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AllocatedIdKind {
    /// A run.
    RunId,
    /// A turn within a run (`run_id ⊃ turn_id`).
    TurnId,
    /// A model call within a turn.
    ModelCallId,
    /// A tool call within a model call.
    ToolCallId,
    /// A ledger event.
    EventId,
}

impl AllocatedIdKind {
    /// The nesting parent, if any (`run_id ⊃ turn_id ⊃ model_call_id ⊃ tool_call_id`).
    pub fn parent(self) -> Option<AllocatedIdKind> {
        match self {
            AllocatedIdKind::RunId => None,
            AllocatedIdKind::TurnId => Some(AllocatedIdKind::RunId),
            AllocatedIdKind::ModelCallId => Some(AllocatedIdKind::TurnId),
            AllocatedIdKind::ToolCallId => Some(AllocatedIdKind::ModelCallId),
            AllocatedIdKind::EventId => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_tags_are_distinct_per_kind() {
        // N3: no two record kinds share a domain tag, so equal bytes never collide across kinds.
        let kinds = [
            RecordKind::HirNode,
            RecordKind::HirEdge,
            RecordKind::HirTextLeaf,
            RecordKind::SealedDefinition,
            RecordKind::VariantRecord,
            RecordKind::ModelProfile,
            RecordKind::PermissionPolicy,
            RecordKind::Budget,
            RecordKind::Validator,
            RecordKind::TaskDataManifest,
            RecordKind::CapabilityDeclaration,
            RecordKind::RunManifest,
            RecordKind::BundleManifest,
            RecordKind::NameBindingRecord,
            RecordKind::IdentityProfile,
            RecordKind::ExtensionRecord,
            RecordKind::RegistryRecord,
            RecordKind::RevocationRecord,
            RecordKind::Configuration,
            RecordKind::ConfigurationVersion,
            RecordKind::ContainmentPolicy,
        ];
        let mut seen = std::collections::BTreeSet::new();
        for k in kinds {
            assert!(seen.insert(k.domain_tag()), "duplicate domain tag {:?}", k);
            // The tag must not contain the framing separator (idp_digest relies on this).
            assert!(!k.domain_tag().contains('\u{1f}'));
        }
    }

    #[test]
    fn sealed_forms_are_exactly_the_pinned_kinds() {
        assert!(RecordKind::SealedDefinition.is_sealed_form());
        assert!(RecordKind::RunManifest.is_sealed_form());
        assert!(RecordKind::BundleManifest.is_sealed_form());
        assert!(RecordKind::RegistryRecord.is_sealed_form());
        assert!(!RecordKind::HirNode.is_sealed_form());
        assert!(!RecordKind::NameBindingRecord.is_sealed_form());
    }

    #[test]
    fn name_binding_and_registry_have_no_semantic_projection() {
        // §8.3 #2: surface/provenance/version kinds are version-only.
        assert!(!RecordKind::NameBindingRecord.has_semantic_projection());
        assert!(!RecordKind::RegistryRecord.has_semantic_projection());
        assert!(!RecordKind::RevocationRecord.has_semantic_projection());
        assert!(RecordKind::HirNode.has_semantic_projection());
        assert!(RecordKind::SealedDefinition.has_semantic_projection());
    }

    #[test]
    fn allocated_id_nesting_is_the_spec_chain() {
        assert_eq!(
            AllocatedIdKind::ToolCallId.parent(),
            Some(AllocatedIdKind::ModelCallId)
        );
        assert_eq!(
            AllocatedIdKind::ModelCallId.parent(),
            Some(AllocatedIdKind::TurnId)
        );
        assert_eq!(
            AllocatedIdKind::TurnId.parent(),
            Some(AllocatedIdKind::RunId)
        );
        assert_eq!(AllocatedIdKind::RunId.parent(), None);
    }
}
