//! The ownership ledger (ADR-0191 O-1…O-6; `OwnershipRecord` is
//! `no-precedent` — this module is its Stage-4 realization).
//!
//! Ownership is **not** authority: a `control.ownership.transferred` row
//! moves an [`crate::types::OwnedObject`] between holders; authority still
//! gates every effect through the handle set. The projection folds the
//! parent run's `control.ownership.transferred` events plus the declared
//! roots (`manifest`-level seeds passed at construction — a run owns its
//! own workspace tree at open).

use std::collections::BTreeMap;

use hh_ledger::store::Store;
use hh_wire::json::Json;

use crate::types::OwnedObject;
use crate::types::SpawnError;

/// The run-tree's ownership projection — `object → owner` built by folding
/// `control.ownership.transferred` rows over the runs of one store.
/// Revocation of a holder's lease/handles makes its records
/// `return_to_grantor`-eligible (O-5 — the escrow question is ADR-0215's
/// OQ-431 deferral; the *record* keeps `grant_ref` so the grantor chain is
/// auditable now).
#[derive(Debug, Clone, Default)]
pub struct OwnershipTable {
    /// Declared roots — `(owner, object)` seeds that predate any transfer
    /// (a run's own workspace tree at `open_run`; `grant_ref = none`).
    pub roots: Vec<(String, OwnedObject)>,
    /// The folded current holders: `object → owner`.
    current: BTreeMap<OwnedObject, String>,
    /// The transfer history (per object, in event order).
    history: Vec<Json>,
}

impl OwnershipTable {
    /// Build the projection for one run's view: declared roots + every
    /// `control.ownership.transferred` row visible across the store's runs
    /// (a child sees the parent's grants to it; transfers are ledger facts,
    /// not run-local state — the fold is over the whole store so a sibling
    /// relay sees the same table).
    pub fn project(
        store: &Store,
        run_ids: &[String],
        roots: Vec<(String, OwnedObject)>,
    ) -> Result<OwnershipTable, SpawnError> {
        let mut t = OwnershipTable {
            roots,
            current: BTreeMap::new(),
            history: Vec::new(),
        };
        for (owner, obj) in t.roots.clone() {
            t.current.insert(obj, owner);
        }
        for run_id in run_ids {
            let events = store
                .events(run_id)
                .map_err(|e| SpawnError::Kernel(e.to_string()))?;
            for e in events {
                if e.class != "control.ownership.transferred" {
                    continue;
                }
                let obj = e.payload.get("object").and_then(OwnedObject::from_json);
                let to = e.payload.get("to").and_then(Json::as_str);
                if let (Some(o), Some(to)) = (obj, to) {
                    t.current.insert(o, to.to_string());
                    t.history.push(e.payload.clone());
                }
            }
        }
        Ok(t)
    }

    /// Whether `holder` owns `object` — exact or by containment (a holder
    /// of `/src` owns `/src/a.rs`).
    pub fn holds(&self, holder: &str, object: &OwnedObject) -> bool {
        self.current
            .iter()
            .any(|(o, owner)| owner == holder && o.covers(object))
    }

    /// The current holder of `object` (nearest covering record — a
    /// transferred `/src` beats the `/` root the grant descended from;
    /// BTreeMap order is `{kind, key}` lexicographic, so rank by the
    /// covering record's specificity, not iteration order).
    pub fn owner_of(&self, object: &OwnedObject) -> Option<&str> {
        self.current
            .iter()
            .filter(|(o, _)| o.covers(object))
            .max_by_key(|(o, _)| match o {
                OwnedObject::FsPathPrefix(p) => p.len(),
                OwnedObject::ResourceKey(k) => k.len(),
            })
            .map(|(_, owner)| owner.as_str())
    }

    /// Every object `holder` currently owns.
    pub fn owned_by(&self, holder: &str) -> Vec<OwnedObject> {
        self.current
            .iter()
            .filter(|(_, owner)| owner.as_str() == holder)
            .map(|(o, _)| o.clone())
            .collect()
    }

    /// The transfer history rows (audit).
    pub fn history(&self) -> &[Json] {
        &self.history
    }
}
