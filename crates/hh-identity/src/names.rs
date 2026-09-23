//! The **local single-namespace name index** with `publish`/`resolve` (§8.3 #2/#9, C0/Stage 1;
//! ADR-0037 D1). A name is a *mutable pointer with an append-only history* (I2, N4) — never an
//! identity. Stage 1 lands the local index and the two Stage-1 namespaces (`hh/` kernel-owned,
//! `local/`); the multi-namespace registry *service*, `exp/` and shared reverse-DNS namespaces
//! are the C1 registry (S4.1 / §06).

use hh_provenance::{AuthorityClass, ProvenanceRecord};

use crate::kinds::RecordKind;
use crate::refs::{Name, NameSelector, VersionedRef};

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
    /// The publisher's canonical provenance record (§8.1; DF-S1.2-1 — `publish`'s rules
    /// read its `AuthorityClass`).
    pub publisher: ProvenanceRecord,
    /// The transaction-time sequence (ordering uses only this `seq`, never a wall clock — §8.3 #2).
    pub published_at_seq: u64,
    /// The content id of the `supersedes` diff (`idp("registry.diff", H(ops))`) — the
    /// §8.3 #3 `diff_ref` a successor publish mints over the superseded body. `None`
    /// for a first publish or a status-only entry (deprecate/yank carry no diff).
    pub diff_ref: Option<String>,
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
    /// Publishing to a kernel-owned namespace without `kernel`-authority provenance.
    NamespaceForbidden { namespace: Namespace },
    /// A widening/loosening successor without a `principal`-authority attested provenance
    /// (§8.3 #6; ADR-0037 D5 — the §8.1 `AuthorityClass` + attestation rule).
    AuthorityWideningRequiresHuman,
    /// A widening/loosening successor without the required **MAJOR label bump**
    /// (§8.3 #6: widening produces a `diff_ref` *and* a MAJOR bump — the label
    /// must parse `v?<major>[.<minor>[.<patch>]]` and its major must exceed the
    /// superseded entry's).
    LabelBumpRequired {
        /// The name whose publish refused.
        name: String,
        /// The label that was required to bump (or `none`).
        detail: String,
    },
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
// `Resolved` carries the full `VersionedRef` (whose canonical `ProvenanceRecord` is large);
// boxing the variant would change the public enum's shape for no hot-path gain — the swap to
// the canonical record is additive (CC8), so the lint is allowed here.
#[allow(clippy::large_enum_variant)]
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
    /// uniqueness, kind match with `supersedes`, namespace policy and the widening rule —
    /// a widening/loosening successor requires `principal`+attested provenance **and** a MAJOR
    /// label bump over the superseded entry (§8.3 #6; AC-R-2.12.2-13).
    #[allow(clippy::too_many_arguments)]
    pub fn publish(
        &mut self,
        namespace: Namespace,
        name: &str,
        version_ref: &VersionedRef,
        label: Option<String>,
        supersedes: Option<&NameHistoryEntry>,
        status: NameStatus,
        publisher: ProvenanceRecord,
        widening: bool,
        diff_ref: Option<String>,
    ) -> Result<NameHistoryEntry, PublishError> {
        // `hh/` is kernel-owned: the publisher's *conferred authority* must be `kernel`
        // (§8.1 — the class, not a self-declared origin string).
        if namespace == Namespace::Hh && publisher.authority != AuthorityClass::Kernel {
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
        // Widening/loosening successor: `authority == principal` **and** an attestation
        // (§8.3 #6 reads the canonical `AuthorityClass` — DF-S1.2-1). A human principal mints
        // `principal`; the attestation is the verified fact, never a self-declared flag.
        if widening
            && !(publisher.authority == AuthorityClass::Principal
                && publisher.attestation.is_some())
        {
            return Err(PublishError::AuthorityWideningRequiresHuman);
        }
        if widening {
            // §8.3 #6 / AC-R-2.12.2-13: a widening successor bumps the MAJOR
            // label component. Both labels parse `v?<major>[.<minor>[.<patch>]]`;
            // an absent or unparsable label can never evidence the bump.
            let bump_ok = match (
                label_major(label.as_deref()),
                supersedes.and_then(|s| label_major(s.label.as_deref())),
            ) {
                (Some(new_major), Some(old_major)) => new_major > old_major,
                _ => false,
            };
            if !bump_ok {
                return Err(PublishError::LabelBumpRequired {
                    name: name.to_string(),
                    detail: label.clone().unwrap_or_else(|| "none".to_string()),
                });
            }
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
            diff_ref,
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
                // Execute never returns a yanked head — and a yank poisons the
                // *version* it binds, not just the entry's position (S1.8 /
                // ADR-0239: otherwise re-binding the same version under the name
                // would resurrect a pulled version — `deprecate`/`yank` must have
                // observable effect). Fall back to the latest entry binding a
                // version no yanked entry names.
                let poisoned: std::collections::BTreeSet<&str> = hist
                    .iter()
                    .filter(|e| e.status == NameStatus::Yanked)
                    .map(|e| e.version_id.as_str())
                    .collect();
                match hist.iter().rev().find(|e| {
                    e.status != NameStatus::Yanked && !poisoned.contains(e.version_id.as_str())
                }) {
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

/// The MAJOR component of a SemVer-class label (`v?MAJOR[.MINOR[.PATCH]]`).
/// `None` for anything else — a label that cannot evidence a bump is not a
/// label the widening gate accepts.
fn label_major(label: Option<&str>) -> Option<u64> {
    let l = label?.strip_prefix('v').unwrap_or(label?);
    let major = l.split('.').next()?;
    if major.is_empty() || !major.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    major.parse::<u64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_provenance::{HumanRole, Origin, PersistenceScope};

    /// A delegate-class (agent/model) publisher — the canonical record, `delegate` authority.
    fn agent() -> ProvenanceRecord {
        ProvenanceRecord::minted(
            Origin::model("model:m", "run:r", "resp:1"),
            PersistenceScope::Run,
            0,
        )
    }

    /// A `kernel`-authority publisher.
    fn kernel() -> ProvenanceRecord {
        ProvenanceRecord::kernel("kernel:identity", 0)
    }

    /// A `principal`-authority attested publisher (widening-successor provenance).
    fn human_attested() -> ProvenanceRecord {
        ProvenanceRecord::human_attested(
            "alice",
            HumanRole::Principal,
            PersistenceScope::User,
            0,
            crate::refs::test_attestation(),
        )
    }

    fn vref(kind: RecordKind, vid: &str, sem: Option<&str>) -> VersionedRef {
        let mut r = VersionedRef::pinned(kind, vid, agent());
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
                agent(),
                false,
                None,
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
            agent(),
            false,
            None,
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
            agent(),
            false,
            None,
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
                agent(),
                false,
                None
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
                agent(),
                false,
                None
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
                kernel(),
                false,
                None
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
                agent(),
                true,
                None
            ),
            Err(PublishError::AuthorityWideningRequiresHuman)
        ));
        // Attested but no label bump evidence → LabelBumpRequired.
        assert!(matches!(
            idx.publish(
                Namespace::Local,
                "p",
                &v,
                None,
                None,
                NameStatus::Active,
                human_attested(),
                true,
                None
            ),
            Err(PublishError::LabelBumpRequired { .. })
        ));
    }

    #[test]
    fn widening_successor_requires_a_major_label_bump() {
        // AC-R-2.12.2-13 (§8.3 #6): widening ⇒ principal+attestation AND a MAJOR bump.
        let mut idx = NameIndex::new();
        let v1 = vref(RecordKind::PermissionPolicy, "sha256:v1", None);
        let e1 = idx
            .publish(
                Namespace::Local,
                "p",
                &v1,
                Some("1.2.0".into()),
                None,
                NameStatus::Active,
                human_attested(),
                false,
                None,
            )
            .unwrap();
        let v2 = vref(RecordKind::PermissionPolicy, "sha256:v2", None);
        // Attested, widening, but only a minor bump → LabelBumpRequired.
        assert!(matches!(
            idx.publish(
                Namespace::Local,
                "p",
                &v2,
                Some("1.3.0".into()),
                Some(&e1),
                NameStatus::Active,
                human_attested(),
                true,
                None
            ),
            Err(PublishError::LabelBumpRequired { .. })
        ));
        // Attested + MAJOR bump → ok (the `diff_ref` rides the entry).
        let e2 = idx
            .publish(
                Namespace::Local,
                "p",
                &v2,
                Some("2.0.0".into()),
                Some(&e1),
                NameStatus::Active,
                human_attested(),
                true,
                Some("sha256:diff".into()),
            )
            .unwrap();
        assert_eq!(e2.diff_ref.as_deref(), Some("sha256:diff"));
        // A non-widening successor never needs the bump.
        let v3 = vref(RecordKind::PermissionPolicy, "sha256:v3", None);
        assert!(idx
            .publish(
                Namespace::Local,
                "p",
                &v3,
                Some("2.0.1".into()),
                Some(&e2),
                NameStatus::Active,
                agent(),
                false,
                None,
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
                agent(),
                false,
                None,
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
            agent(),
            false,
            None,
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
