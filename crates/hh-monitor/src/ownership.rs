//! The ownership ledger's monitor half — check 8 (`check_write`) of the
//! prepare-time checks (§5e.5; ADR-0191 O-1…O-6; ADR-0343 D3). The
//! machinery lives in the Π crate so the seven-stage dispatcher
//! (`hh-env`, a lower tier than `hh-subagent`) can run it at `prepare`
//! without a dependency cycle — `hh-subagent` re-exports these items and
//! keeps the verb half (`transfer_ownership`/`return_ownership`/
//! `record_write_refusal`/`release_resource_locks`) that emits the rows
//! this module folds.
//!
//! Ownership is **not** authority: a `control.ownership.transferred` row
//! moves an [`OwnedObject`] between holders; authority still gates every
//! effect through the handle set. The projection folds the parent run's
//! `control.ownership.transferred` events plus the declared roots
//! (`manifest`-level seeds passed at construction — a run owns its own
//! workspace tree at open).

use std::collections::BTreeMap;

use hh_ledger::errors::LedgerError;
use hh_ledger::store::Store;
use hh_wire::json::Json;

/// `OwnedObject` — a coordination object an ownership record names. The closed
/// sum at this slice: `{fs_path_prefix | resource_key}` — environment writes
/// merge against `fs_path_prefix` ownership (the `NotOwner` check); declared
/// resource keys cover `reserved_keys` objects.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum OwnedObject {
    /// A workspace path prefix (merge/NotOwner territory).
    FsPathPrefix(String),
    /// A named resource key (share/reservation territory).
    ResourceKey(String),
}

impl OwnedObject {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        match self {
            OwnedObject::FsPathPrefix(p) => Json::obj([
                ("kind", Json::str("fs_path_prefix")),
                ("key", Json::str(p.clone())),
            ]),
            OwnedObject::ResourceKey(k) => Json::obj([
                ("kind", Json::str("resource_key")),
                ("key", Json::str(k.clone())),
            ]),
        }
    }

    /// From canonical JSON.
    pub fn from_json(j: &Json) -> Option<OwnedObject> {
        let key = j.get("key")?.as_str()?.to_string();
        Some(match j.get("kind")?.as_str()? {
            "fs_path_prefix" => OwnedObject::FsPathPrefix(key),
            "resource_key" => OwnedObject::ResourceKey(key),
            _ => return None,
        })
    }

    /// Whether `self` covers `other` (`⊆` — a prefix owns its descendants).
    pub fn covers(&self, other: &OwnedObject) -> bool {
        match (self, other) {
            (OwnedObject::FsPathPrefix(a), OwnedObject::FsPathPrefix(b)) => {
                a == "/" || a.is_empty() || b == a || {
                    let mut p = a.clone();
                    if !p.ends_with('/') {
                        p.push('/');
                    }
                    b.starts_with(&p)
                }
            }
            (OwnedObject::ResourceKey(a), OwnedObject::ResourceKey(b)) => a == b,
            _ => false,
        }
    }
}

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
    ) -> Result<OwnershipTable, LedgerError> {
        let mut t = OwnershipTable {
            roots,
            current: BTreeMap::new(),
            history: Vec::new(),
        };
        for (owner, obj) in t.roots.clone() {
            t.current.insert(obj, owner);
        }
        for run_id in run_ids {
            let events = store.events(run_id)?;
            for e in events {
                match e.class.as_str() {
                    // `transferred`/`returned` carry a single `object` +
                    // `to` (the O-5 return restores the grantor as `to`).
                    "control.ownership.transferred" | "control.ownership.returned" => {
                        let obj = e.payload.get("object").and_then(OwnedObject::from_json);
                        let to = e.payload.get("to").and_then(Json::as_str);
                        if let (Some(o), Some(to)) = (obj, to) {
                            t.current.insert(o, to.to_string());
                            t.history.push(e.payload.clone());
                        }
                    }
                    // `granted` carries `objects[]` + `to` — every member
                    // moves to the grantee (spawn-time conferral).
                    "control.ownership.granted" => {
                        let to = e.payload.get("to").and_then(Json::as_str);
                        if let (Some(Json::Arr(objs)), Some(to)) = (e.payload.get("objects"), to) {
                            for o in objs {
                                if let Some(o) = OwnedObject::from_json(o) {
                                    t.current.insert(o, to.to_string());
                                }
                            }
                            t.history.push(e.payload.clone());
                        }
                    }
                    _ => {}
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

// ─────────────────────────────────────────────────────────────────────────────
// check 8 — the prepare-time ownership monitor (§5e.5; ADR-0191 O-3)
// ─────────────────────────────────────────────────────────────────────────────

/// The monitor check-8 verdict — `ok | NotOwner{owner} | Fenced` (§5e.5
/// `check_write(effect_id, object)`; an unowned or stale-generation write
/// is a typed refusal, never applied).
#[derive(Debug, Clone, PartialEq)]
pub enum WriteVerdict {
    /// The write may prepare.
    Ok,
    /// Another holder owns the object (or a `resource` write has no held
    /// lease — `owner` names the live holder, `"unheld"` the missing one).
    NotOwner { owner: String },
    /// The writer's own lease/record is stale (a fenced scoped lease, or
    /// the record minted under a superseded writer generation).
    Fenced { detail: String },
}

/// `check_write(store, writer_run_id, writer_generation, object,
/// ownerships)` — the pure half of monitor check 8: the writer may apply
/// an effect to `object` iff the ownership projection names it *or* (for
/// `resource{key}` objects) a live `resource:<key>` scoped lease names it.
///
/// - `FsPathPrefix` — the table's `owner_of` decides; a covering record
///   owned by another run is `NotOwner`.
/// - `ResourceKey` — the cross-run `scoped_lease_holders` scan decides:
///   the writer's own live record passes (`holder` may spell the run id
///   or `subagent:<run>` — the spawn-time holder convention); a record
///   minted under a *superseded* writer generation is `Fenced`; another
///   holder is `NotOwner{holder}`; no holder at all is
///   `NotOwner{unheld}` (a shared key may not be written lock-free).
pub fn check_write(
    store: &Store,
    writer_run_id: &str,
    writer_generation: u64,
    object: &OwnedObject,
    ownerships: &OwnershipTable,
) -> Result<WriteVerdict, LedgerError> {
    // The transfer/grant projection binds first — an object the table
    // says the writer owns clears without a resource lease (the grant
    // rows are the ledgered basis, ADR-0191 O-2).
    match ownerships.owner_of(object) {
        Some(o) if o == writer_run_id => return Ok(WriteVerdict::Ok),
        Some(o) => {
            return Ok(WriteVerdict::NotOwner {
                owner: o.to_string(),
            })
        }
        None => {}
    }
    if let OwnedObject::ResourceKey(key) = object {
        let scope = hh_ledger::leases::LeaseScope::Resource(key.clone());
        // The writer's own record — a superseded generation fences the
        // write (the fence's whole point: a stale writer may not act).
        if let Some(rec) = store.scoped_lease(writer_run_id, &scope)? {
            if rec.generation < writer_generation {
                return Ok(WriteVerdict::Fenced {
                    detail: format!(
                        "resource lease generation {} < writer generation {}",
                        rec.generation, writer_generation
                    ),
                });
            }
            if rec.holder == writer_run_id || rec.holder == format!("subagent:{writer_run_id}") {
                return Ok(WriteVerdict::Ok);
            }
            return Ok(WriteVerdict::NotOwner { owner: rec.holder });
        }
        // The cross-run scan — a live holder anywhere contends.
        if let Some((_run, rec)) = store.scoped_lease_holders(&scope).into_iter().next() {
            if rec.holder == writer_run_id || rec.holder == format!("subagent:{writer_run_id}") {
                return Ok(WriteVerdict::Ok);
            }
            return Ok(WriteVerdict::NotOwner { owner: rec.holder });
        }
        return Ok(WriteVerdict::NotOwner {
            owner: "unheld".to_string(),
        });
    }
    // `fs_path_prefix` unowned: no covering record and no table entry —
    // the caller's roots decide (an unowned subtree is free territory;
    // check 8 governs owned/shared objects, not virgin paths).
    Ok(WriteVerdict::Ok)
}
