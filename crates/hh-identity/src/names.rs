//! The **local single-namespace name index** with `publish`/`resolve` (§8.3 #2/#9, C0/Stage 1;
//! ADR-0037 D1). A name is a *mutable pointer with an append-only history* (I2, N4) — never an
//! identity. Stage 1 lands the local index and the two Stage-1 namespaces (`hh/` kernel-owned,
//! `local/`); the multi-namespace registry *service*, `exp/` and shared reverse-DNS namespaces
//! are the C1 registry (S4.1 / §06).

use crate::kinds::RecordKind;
use crate::refs::{Name, NameSelector, Provenance, VersionedRef};

/// A namespace (Stage-1 subset). `hh/` is kernel-owned (publishing to it requires a kernel-origin
/// provenance); `local/` is the working namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Namespace {
    /// Kernel-owned.
    Hh,
    /// Local working namespace.
    Local,
}

impl Namespace {
    pub fn as_str(self) -> &'static str {
        match self {
            Namespace::Hh => "hh",
            Namespace::Local => "local",
        }
    }
    pub fn parse(s: &str) -> Option<Namespace> {
        match s {
            "hh" => Some(Namespace::Hh),
            "local" => Some(Namespace::Local),
            _ => None,
        }
    }
}

/// The status of a name-history entry (§8.3 #3). `yanked` hides from default resolution and never
/// deletes (yank ≠ delete — AC-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameStatus {
    Active,
    Deprecated,
    Yanked,
}

/// One append-only name-history entry (§8.3 #3). `label` is a SemVer-class claim, unique per name;
/// version labels are never identity (N4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameHistoryEntry {
    pub namespace: Namespace,
    pub name: String,
    pub version_id: String,
    pub semantic_id: Option<String>,
    pub kind: RecordKind,
    pub label: Option<String>,
    pub status: NameStatus,
    pub supersedes_entry: Option<String>,
    pub publisher: Provenance,
    /// The transaction-time sequence (ordering uses only this `seq`, never a wall clock — §8.3 #2).
    pub published_at_seq: u64,
}

/// `publish` failure modes (§8.3 #2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishError {
    /// A label already used for this name (labels are unique per name).
    LabelReused { name: String, label: String },
    /// `supersedes` names a version of a different kind.
    KindMismatch {
        expected: RecordKind,
        got: RecordKind,
    },
    /// Publishing to a kernel-owned namespace without a kernel-origin provenance.
    NamespaceForbidden { namespace: Namespace },
    /// A widening/loosening successor without a human-attested provenance (§8.3 #6; ADR-0037 D5).
    AuthorityWideningRequiresHuman,
}

/// `resolve` mode (§8.3 #2). `execute` never returns yanked/revoked heads; `audit`/`reproduce`
/// return the pinned version with its status attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveMode {
    Execute,
    Audit,
    Reproduce,
}

/// The outcome of `resolve` (§8.3 #2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveOutcome {
    Resolved(VersionedRef),
    Unresolved,
    Ambiguous { candidates: Vec<String> },
    Revoked { version_id: String, reason: String },
}

/// The local name index: an append-only list of history entries.
#[derive(Debug, Clone, Default)]
pub struct NameIndex {
    entries: Vec<NameHistoryEntry>,
    next_seq: u64,
}

impl NameIndex {
    pub fn new() -> NameIndex {
        NameIndex {
            entries: Vec::new(),
            next_seq: 0,
        }
    }

    /// All history entries for a `(namespace, name)`, in publish order.
    pub fn history(&self, namespace: Namespace, name: &str) -> Vec<&NameHistoryEntry> {
        self.entries
            .iter()
            .filter(|e| e.namespace == namespace && e.name == name)
            .collect()
    }

    /// `publish(namespace, name, version_id, …) → NameHistoryEntry` (append-only). Enforces label
    /// uniqueness, kind match with `supersedes`, namespace policy and the widening rule.
    #[allow(clippy::too_many_arguments)]
    pub fn publish(
        &mut self,
        namespace: Namespace,
        name: &str,
        version_ref: &VersionedRef,
        label: Option<String>,
        supersedes: Option<&NameHistoryEntry>,
        status: NameStatus,
        publisher: Provenance,
        widening: bool,
    ) -> Result<NameHistoryEntry, PublishError> {
        if namespace == Namespace::Hh && publisher.origin != crate::refs::Origin::Kernel {
            return Err(PublishError::NamespaceForbidden { namespace });
        }
        if let Some(l) = &label {
            if self
                .history(namespace, name)
                .iter()
                .any(|e| e.label.as_deref() == Some(l.as_str()))
            {
                return Err(PublishError::LabelReused {
                    name: name.to_string(),
                    label: l.clone(),
                });
            }
        }
        if let Some(sup) = supersedes {
            if sup.kind != version_ref.kind {
                return Err(PublishError::KindMismatch {
                    expected: sup.kind,
                    got: version_ref.kind,
                });
            }
        }
        if widening && !(publisher.origin == crate::refs::Origin::Human && publisher.attested) {
            return Err(PublishError::AuthorityWideningRequiresHuman);
        }
        let entry = NameHistoryEntry {
            namespace,
            name: name.to_string(),
            version_id: version_ref.version_id.clone(),
            semantic_id: version_ref.semantic_id.clone(),
            kind: version_ref.kind,
            label,
            status,
            supersedes_entry: supersedes.map(|e| e.version_id.clone()),
            publisher,
            published_at_seq: self.next_seq,
        };
        self.next_seq += 1;
        self.entries.push(entry.clone());
        Ok(entry)
    }

    /// Look up an entry by its exact `version_id` (a pinned id resolves in every mode with its
    /// status attached — §8.3 #2).
    pub fn by_version_id(&self, version_id: &str) -> Option<&NameHistoryEntry> {
        self.entries.iter().find(|e| e.version_id == version_id)
    }

    /// `resolve(selector, mode)`. Deterministic given the index state. `execute` never returns a
    /// `yanked` head (revocation enforcement over the `StaleIndex` is layered in by
    /// [`crate::supersede`] at Stage 2). The head is the latest `active` entry by `seq`.
    pub fn resolve(&self, selector: &NameSelector, mode: ResolveMode) -> ResolveOutcome {
        let ns = match Namespace::parse(&selector.namespace) {
            Some(ns) => ns,
            None => return ResolveOutcome::Unresolved,
        };
        let mut hist: Vec<&NameHistoryEntry> = self.history(ns, &selector.name);
        if hist.is_empty() {
            return ResolveOutcome::Unresolved;
        }
        // If a label is requested, filter to it (unique per name).
        if let Some(label) = &selector.label {
            hist.retain(|e| e.label.as_deref() == Some(label.as_str()));
            if hist.is_empty() {
                return ResolveOutcome::Unresolved;
            }
        }
        hist.sort_by_key(|e| e.published_at_seq);
        let head = *hist.last().unwrap();
        match (mode, head.status) {
            (ResolveMode::Execute, NameStatus::Yanked) => {
                // Execute never returns a yanked head; fall back to the latest non-yanked.
                match hist.iter().rev().find(|e| e.status != NameStatus::Yanked) {
                    Some(e) => ResolveOutcome::Resolved(self.to_ref(e, selector)),
                    None => ResolveOutcome::Unresolved,
                }
            }
            _ => ResolveOutcome::Resolved(self.to_ref(head, selector)),
        }
    }

    fn to_ref(&self, e: &NameHistoryEntry, selector: &NameSelector) -> VersionedRef {
        VersionedRef {
            kind: e.kind,
            version_id: e.version_id.clone(),
            semantic_id: e.semantic_id.clone(),
            name: Some(Name {
                namespace: e.namespace.as_str().to_string(),
                name: e.name.clone(),
                label: e.label.clone(),
            }),
            resolved_from: Some(selector.clone()),
            supersedes: e.supersedes_entry.clone(),
            provenance: e.publisher.clone(),
            idp: crate::idp::IDP_1.idp_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refs::Origin;

    fn vref(kind: RecordKind, vid: &str, sem: Option<&str>) -> VersionedRef {
        let mut r = VersionedRef::pinned(kind, vid, Provenance::new(Origin::Agent));
        r.semantic_id = sem.map(|s| s.to_string());
        r
    }

    #[test]
    fn publish_is_append_only_and_resolves_the_head() {
        let mut idx = NameIndex::new();
        let v1 = vref(RecordKind::VariantRecord, "sha256:v1", Some("sha256:s"));
        let e1 = idx
            .publish(
                Namespace::Local,
                "tool/grep",
                &v1,
                Some("1.0.0".into()),
                None,
                NameStatus::Active,
                Provenance::new(Origin::Agent),
                false,
            )
            .unwrap();
        let v2 = vref(RecordKind::VariantRecord, "sha256:v2", Some("sha256:s"));
        idx.publish(
            Namespace::Local,
            "tool/grep",
            &v2,
            Some("1.1.0".into()),
            Some(&e1),
            NameStatus::Active,
            Provenance::new(Origin::Agent),
            false,
        )
        .unwrap();
        assert_eq!(idx.history(Namespace::Local, "tool/grep").len(), 2);
        match idx.resolve(
            &NameSelector::new("local", "tool/grep"),
            ResolveMode::Execute,
        ) {
            ResolveOutcome::Resolved(r) => assert_eq!(r.version_id, "sha256:v2"),
            o => panic!("expected head v2, got {o:?}"),
        }
    }

    #[test]
    fn label_reuse_is_rejected() {
        let mut idx = NameIndex::new();
        let v1 = vref(RecordKind::VariantRecord, "sha256:v1", None);
        idx.publish(
            Namespace::Local,
            "n",
            &v1,
            Some("1.0.0".into()),
            None,
            NameStatus::Active,
            Provenance::new(Origin::Agent),
            false,
        )
        .unwrap();
        let v2 = vref(RecordKind::VariantRecord, "sha256:v2", None);
        assert!(matches!(
            idx.publish(
                Namespace::Local,
                "n",
                &v2,
                Some("1.0.0".into()),
                None,
                NameStatus::Active,
                Provenance::new(Origin::Agent),
                false
            ),
            Err(PublishError::LabelReused { .. })
        ));
    }

    #[test]
    fn hh_namespace_requires_kernel_origin() {
        let mut idx = NameIndex::new();
        let v1 = vref(RecordKind::VariantRecord, "sha256:v1", None);
        assert!(matches!(
            idx.publish(
                Namespace::Hh,
                "n",
                &v1,
                None,
                None,
                NameStatus::Active,
                Provenance::new(Origin::Agent),
                false
            ),
            Err(PublishError::NamespaceForbidden { .. })
        ));
        assert!(idx
            .publish(
                Namespace::Hh,
                "n",
                &v1,
                None,
                None,
                NameStatus::Active,
                Provenance::kernel(),
                false
            )
            .is_ok());
    }

    #[test]
    fn widening_successor_requires_human_attestation() {
        let mut idx = NameIndex::new();
        let v = vref(RecordKind::PermissionPolicy, "sha256:v1", None);
        assert!(matches!(
            idx.publish(
                Namespace::Local,
                "p",
                &v,
                None,
                None,
                NameStatus::Active,
                Provenance::new(Origin::Agent),
                true
            ),
            Err(PublishError::AuthorityWideningRequiresHuman)
        ));
        assert!(idx
            .publish(
                Namespace::Local,
                "p",
                &v,
                None,
                None,
                NameStatus::Active,
                Provenance::human_attested(),
                true
            )
            .is_ok());
    }

    #[test]
    fn execute_mode_never_returns_a_yanked_head() {
        // AC-11 shape: yanked hides from default (execute) resolution; the entry still exists.
        let mut idx = NameIndex::new();
        let v1 = vref(RecordKind::VariantRecord, "sha256:v1", None);
        let e1 = idx
            .publish(
                Namespace::Local,
                "n",
                &v1,
                Some("1.0.0".into()),
                None,
                NameStatus::Active,
                Provenance::new(Origin::Agent),
                false,
            )
            .unwrap();
        let v2 = vref(RecordKind::VariantRecord, "sha256:v2", None);
        idx.publish(
            Namespace::Local,
            "n",
            &v2,
            Some("1.1.0".into()),
            Some(&e1),
            NameStatus::Yanked,
            Provenance::new(Origin::Agent),
            false,
        )
        .unwrap();
        match idx.resolve(&NameSelector::new("local", "n"), ResolveMode::Execute) {
            ResolveOutcome::Resolved(r) => {
                assert_eq!(r.version_id, "sha256:v1", "execute skips the yanked head")
            }
            o => panic!("unexpected {o:?}"),
        }
        // Audit still sees the yanked head, and the version is retrievable by id (yank ≠ delete).
        assert!(idx.by_version_id("sha256:v2").is_some());
        match idx.resolve(&NameSelector::new("local", "n"), ResolveMode::Audit) {
            ResolveOutcome::Resolved(r) => assert_eq!(r.version_id, "sha256:v2"),
            o => panic!("unexpected {o:?}"),
        }
    }
}
