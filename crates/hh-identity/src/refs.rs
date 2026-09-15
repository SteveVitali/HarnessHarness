//! [`VersionedRef`] — the one reference type every consumer uses (§8.3 #3; ADR-0036 D1) — plus
//! the `NameSelector` and the canonical provenance hand-off.
//!
//! Provenance note (CC1 / CC10). Provenance & authority are owned by **§8.1 / R-2.1.5** and land
//! at **S1.3** in `hh-provenance`. Identity *carries* a provenance on every [`VersionedRef`] and
//! name-history entry, and it is the **canonical [`ProvenanceRecord`]** — this crate does not
//! define a second record (CC1). DF-S1.2-1 is closed: the interim `Origin`/`Provenance` view is
//! replaced by `hh_provenance::{Origin, ProvenanceRecord}` and the widening-successor rule in
//! [`crate::names`] reads the real `AuthorityClass` (§8.1), not the interim `attested` flag.

use crate::kinds::RecordKind;

// The canonical provenance types (§8.1 / R-2.1.5) — re-exported so `hh_identity::refs::Origin`
// and `hh_identity::ProvenanceRecord` remain the paths consumers use. `hh-provenance` sits
// *below* this crate in the build graph (a `ProvenanceRecord` carries identity coordinates as
// strings; the typed `Ref<T>` projection lands at S1.4 — ADR-0230).
pub use hh_provenance::{HumanRole, Origin, ProvenanceRecord};

/// Back-compatible name for the canonical record (CC8 additive): the Stage-1 `Provenance`
/// wrapper is gone; this alias keeps `hh_identity::Provenance` resolving to the one canonical
/// type (never a second scheme — it *is* `ProvenanceRecord`).
pub type Provenance = ProvenanceRecord;

/// A **name selector** — a mutable pointer *not* an identity (N4). A selector inside a record to
/// be sealed is refused (`UnresolvedRef`; N5). Resolving one records what it resolved to in
/// `resolved_from`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameSelector {
    pub namespace: String,
    pub name: String,
    /// An optional SemVer-class label constraint (never identity).
    pub label: Option<String>,
}

impl NameSelector {
    pub fn new(namespace: impl Into<String>, name: impl Into<String>) -> NameSelector {
        NameSelector {
            namespace: namespace.into(),
            name: name.into(),
            label: None,
        }
    }
}

/// The **one reference type** every consumer uses (§8.3 #3). The HIR `Ref` is its projection; a
/// `ContractRef` (08.4) is the dependency form over it. `version_id` is the execution coordinate,
/// `semantic_id` the comparison coordinate (N6). `provenance` is the canonical §8.1
/// [`ProvenanceRecord`] — the field's role is unchanged (who produced this version), only its
/// type is now the canonical record (DF-S1.2-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedRef {
    pub kind: RecordKind,
    pub version_id: String,
    pub semantic_id: Option<String>,
    pub name: Option<Name>,
    pub resolved_from: Option<NameSelector>,
    pub supersedes: Option<String>,
    pub provenance: ProvenanceRecord,
    pub idp: &'static str,
}

/// The `{namespace, name, label?}` a `VersionedRef` may carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Name {
    pub namespace: String,
    pub name: String,
    pub label: Option<String>,
}

impl VersionedRef {
    /// A minimal pinned reference (kind + version id + provenance), the shape a sealed form
    /// embeds. `idp` is fixed to `idp/1` (the one mandatory-writable profile).
    pub fn pinned(
        kind: RecordKind,
        version_id: impl Into<String>,
        provenance: ProvenanceRecord,
    ) -> VersionedRef {
        VersionedRef {
            kind,
            version_id: version_id.into(),
            semantic_id: None,
            name: None,
            resolved_from: None,
            supersedes: None,
            provenance,
            idp: crate::idp::IDP_1.idp_id,
        }
    }

    pub fn with_semantic(mut self, semantic_id: impl Into<String>) -> VersionedRef {
        self.semantic_id = Some(semantic_id.into());
        self
    }
}

/// A verified attestation shared by the provenance-touching tests (hash-chain anchored).
#[cfg(test)]
pub(crate) fn test_attestation() -> hh_provenance::Attestation {
    hh_provenance::Attestation {
        kind: hh_provenance::AttestationKind::HashChain,
        subject_hash: "sha256:sub".into(),
        anchor: hh_provenance::AttestationAnchor::Chain("sha256:head".into()),
        verified_by: "kernel:chain".into(),
        verified_at: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_provenance::PersistenceScope;

    #[test]
    fn versioned_ref_pins_idp_and_version() {
        let r = VersionedRef::pinned(
            RecordKind::VariantRecord,
            "sha256:abc",
            ProvenanceRecord::kernel("kernel:identity", 0),
        )
        .with_semantic("sha256:sem");
        assert_eq!(r.idp, "idp/1");
        assert_eq!(r.version_id, "sha256:abc");
        assert_eq!(r.semantic_id.as_deref(), Some("sha256:sem"));
        // The provenance is the canonical record: a kernel origin mints `kernel` authority.
        assert_eq!(
            r.provenance.authority,
            hh_provenance::AuthorityClass::Kernel
        );
    }

    #[test]
    fn widening_needs_principal_authority_plus_attestation() {
        // DF-S1.2-1: the widening rule now reads `authority == principal` + an attestation —
        // a human principal with a verified attestation qualifies; an unattested one does not.
        let attested = ProvenanceRecord::human_attested(
            "alice",
            HumanRole::Principal,
            PersistenceScope::User,
            0,
            test_attestation(),
        );
        assert_eq!(attested.authority, hh_provenance::AuthorityClass::Principal);
        assert!(attested.attestation.is_some());
        let unattested = ProvenanceRecord::minted(
            Origin::human("alice", HumanRole::Principal),
            PersistenceScope::User,
            0,
        );
        assert!(unattested.attestation.is_none());
    }
}
