//! [`VersionedRef`] — the one reference type every consumer uses (§8.3 #3; ADR-0036 D1) — plus
//! the `NameSelector` and the Stage-1 `Origin`/`Provenance` view.
//!
//! Provenance note (CC1 / CC10). Provenance & authority are owned by **§8.1 / R-2.1.5** and land
//! at **S1.3**. Identity must *carry* a provenance on every [`VersionedRef`] and name-history
//! entry, but it must not define a *second* `ProvenanceRecord` (CC1). At Stage 1 this crate
//! carries the minimal [`Origin`] the identity rules actually read (`publish` of a widening
//! successor requires `origin = Human`); the canonical `ProvenanceRecord` and its seven
//! `AuthorityClass`es (§8.1) subsume this at S1.3. Tracked by DEFERRALS row DF-S1.2-1.

use crate::kinds::RecordKind;

/// The provenance **origin** an identity operation reads (the identity-slice view of §8.1's
/// `Origin`). Closed here for what Stage-1 identity needs; S1.3 owns the canonical record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Origin {
    /// A human actor with attestation (required to publish a widening/loosening successor).
    Human,
    /// An agent process.
    Agent,
    /// The kernel itself (identity operations are `kernel`-origin facts).
    Kernel,
    /// A deterministic system derivation.
    System,
    /// A value reported by a foreign surface (provider ids, foreign digests — a claim, N7).
    Reported,
}

/// The Stage-1 provenance a [`VersionedRef`] / name entry carries. A thin wrapper over [`Origin`]
/// so the field exists and the widening rule is enforceable now; S1.3 replaces the inner type
/// with the canonical `ProvenanceRecord` without changing this field's role (CC8 additive).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    pub origin: Origin,
    /// Whether an attestation accompanies the record (required with `origin = Human` for a
    /// widening successor; §8.3 #6).
    pub attested: bool,
}

impl Provenance {
    pub fn new(origin: Origin) -> Provenance {
        Provenance {
            origin,
            attested: false,
        }
    }
    pub fn human_attested() -> Provenance {
        Provenance {
            origin: Origin::Human,
            attested: true,
        }
    }
    pub fn kernel() -> Provenance {
        Provenance::new(Origin::Kernel)
    }
}

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
/// `semantic_id` the comparison coordinate (N6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedRef {
    pub kind: RecordKind,
    pub version_id: String,
    pub semantic_id: Option<String>,
    pub name: Option<Name>,
    pub resolved_from: Option<NameSelector>,
    pub supersedes: Option<String>,
    pub provenance: Provenance,
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
        provenance: Provenance,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versioned_ref_pins_idp_and_version() {
        let r = VersionedRef::pinned(
            RecordKind::VariantRecord,
            "sha256:abc",
            Provenance::kernel(),
        )
        .with_semantic("sha256:sem");
        assert_eq!(r.idp, "idp/1");
        assert_eq!(r.version_id, "sha256:abc");
        assert_eq!(r.semantic_id.as_deref(), Some("sha256:sem"));
    }

    #[test]
    fn widening_needs_human_attested_provenance() {
        let p = Provenance::human_attested();
        assert_eq!(p.origin, Origin::Human);
        assert!(p.attested);
    }
}
