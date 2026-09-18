//! The class catalog (§3.3.3; ADR-0023): the `ClassRecord`/`VariantRecord` view the
//! assembly ops consult. Two sources — the Stage-1 built-ins (`hh_registry::suites`'s
//! `control_strategy`/`context_policy` records, the mandatory `exactly-one` classes) and
//! a registry snapshot (`RegistryCatalog` — the snapshot-confined view §6.2 lands). The
//! catalog is a **view**: records live in the registry; this crate never keeps a second
//! store (CC7).

use std::collections::BTreeMap;

use hh_registry::records::{ClassRecord, VariantRecord};
use hh_registry::store::{QueryClause, QueryOp, QueryPredicate, RegistryStore, ResolvedRecord};
use hh_registry::RegistryError;

/// The two Stage-1 mandatory class ids (`exactly-one` — §3.3.3).
pub const STAGE1_CLASSES: &[&str] = &["control_strategy", "context_policy"];

/// The class-record view `validate_assembly`/`resolve`/`instantiate` consult.
pub trait ClassCatalog {
    /// The `ClassRecord` for `class_id`, when the catalog carries one.
    fn class(&self, class_id: &str) -> Option<ClassRecord>;
    /// Every `class_id` the catalog carries (sorted — deterministic iteration).
    fn class_ids(&self) -> Vec<String>;
    /// The `VariantRecord` at a pinned `version_id`, when the catalog can read it
    /// (stage-2's `base ∪ variant` param check; `None` for the built-in catalog).
    fn variant(&self, version_id: &str) -> Option<VariantRecord>;
}

/// The Stage-1 built-in catalog — the two mandatory classes from
/// `hh_registry::suites` (§3.3.3: `control_strategy` and `context_policy` are
/// `exactly-one` at Stage 1).
pub struct Stage1Catalog {
    classes: BTreeMap<String, ClassRecord>,
}

impl Stage1Catalog {
    /// The Stage-1 classes (the two mandatory hot-path classes plus the
    /// S1.21 verification-plane registrations — `validator` ordered-many and
    /// `execution_alignment` floors-only).
    pub fn stage1() -> Stage1Catalog {
        let mut classes = BTreeMap::new();
        for c in [
            hh_registry::suites::control_strategy_class(),
            hh_registry::suites::context_policy_class(),
            hh_registry::suites::validator_class(),
            hh_registry::suites::execution_alignment_class(),
        ] {
            classes.insert(c.class_id.clone(), c);
        }
        Stage1Catalog { classes }
    }
}

impl ClassCatalog for Stage1Catalog {
    fn class(&self, class_id: &str) -> Option<ClassRecord> {
        self.classes.get(class_id).cloned()
    }

    fn class_ids(&self) -> Vec<String> {
        self.classes.keys().cloned().collect()
    }

    fn variant(&self, _version_id: &str) -> Option<VariantRecord> {
        None
    }
}

/// A catalog over a `RegistryStore`, optionally confined to a snapshot (R8 — the
/// snapshot's member set and name bindings). `class(class_id)` returns the record at
/// the deterministically-greatest `version_id` among admissible class records carrying
/// that `class_id` (the catalog read is name-free; ADR-0240).
pub struct RegistryCatalog<'a> {
    store: &'a RegistryStore,
    snapshot_id: Option<String>,
}

impl<'a> RegistryCatalog<'a> {
    /// A catalog view over `store` (snapshot-confined when `snapshot_id` is set).
    pub fn new(store: &'a RegistryStore, snapshot_id: Option<String>) -> RegistryCatalog<'a> {
        RegistryCatalog { store, snapshot_id }
    }

    fn class_records(&self) -> Vec<ResolvedRecord> {
        self.store
            .query(&QueryPredicate {
                clauses: vec![QueryClause {
                    field: "kind".to_string(),
                    op: QueryOp::Eq,
                    value: "class".to_string(),
                }],
                snapshot_id: self.snapshot_id.clone(),
            })
            .unwrap_or_default()
    }
}

impl ClassCatalog for RegistryCatalog<'_> {
    fn class(&self, class_id: &str) -> Option<ClassRecord> {
        let mut found: Vec<(String, ClassRecord)> = self
            .class_records()
            .into_iter()
            .filter_map(|r| match r.record {
                hh_registry::records::RegistryRecord::Class(c) if c.class_id == class_id => {
                    Some((r.versioned_ref.version_id.clone(), c))
                }
                _ => None,
            })
            .collect();
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found.pop().map(|(_, c)| c)
    }

    fn class_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .class_records()
            .into_iter()
            .filter_map(|r| match r.record {
                hh_registry::records::RegistryRecord::Class(c) => Some(c.class_id),
                _ => None,
            })
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    fn variant(&self, version_id: &str) -> Option<VariantRecord> {
        variant_at(self.store, version_id).ok().flatten()
    }
}

/// Look up a `VariantRecord` by pinned `version_id` (instantiate/validate's record
/// reads — through the store, never a second index).
pub fn variant_at(
    store: &RegistryStore,
    version_id: &str,
) -> Result<Option<VariantRecord>, RegistryError> {
    Ok(match store.get(version_id) {
        Some((_, hh_registry::records::RegistryRecord::Variant(v))) => Some(v.clone()),
        _ => None,
    })
}
