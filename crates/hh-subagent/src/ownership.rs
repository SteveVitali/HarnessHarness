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
) -> Result<WriteVerdict, SpawnError> {
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
        if let Some(rec) = store
            .scoped_lease(writer_run_id, &scope)
            .map_err(|e| SpawnError::Kernel(e.to_string()))?
        {
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
        for (_run, rec) in store.scoped_lease_holders(&scope) {
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

/// `record_write_refusal(store, run_id, lease, effect_id, object, verdict)`
/// — the audit half of check 8: `security.policy.evaluated{check:
/// ownership, deny}` under the writer's lease (the refusal is the typed
/// observation AND the audit row, §5e.5). Returns the event id.
pub fn record_write_refusal(
    store: &mut Store,
    run_id: &str,
    lease: &hh_ledger::store::Lease,
    effect_id: &str,
    object: &OwnedObject,
    verdict: &WriteVerdict,
) -> Result<String, SpawnError> {
    let (verdict_str, detail) = match verdict {
        WriteVerdict::Ok => ("allow", Json::Null),
        WriteVerdict::NotOwner { owner } => (
            "deny",
            Json::obj([
                ("reason", Json::str("not_owner")),
                ("owner", Json::str(owner)),
            ]),
        ),
        WriteVerdict::Fenced { detail } => (
            "deny",
            Json::obj([
                ("reason", Json::str("fenced")),
                ("detail", Json::str(detail)),
            ]),
        ),
    };
    let ev = crate::spawn::kernel_ev_pub(
        store,
        run_id,
        "security.policy.evaluated",
        Json::obj([
            ("check", Json::str("ownership")),
            ("verdict", Json::str(verdict_str)),
            ("effect_id", Json::str(effect_id)),
            ("object", object.to_json()),
            ("detail", detail),
        ]),
        vec![],
        None,
    )
    .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    let id = ev.event_id.clone();
    store
        .append(run_id, lease, vec![ev])
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    Ok(id)
}

/// `transfer_ownership(store, run_id, lease, object, from, to, basis,
/// ownerships)` — the §5e.5 verb: `from` must currently hold `object`
/// (an `NotOwned` refusal otherwise); emits
/// `control.ownership.transferred{object, from, to, basis}`.
pub fn transfer_ownership(
    store: &mut Store,
    run_id: &str,
    lease: &hh_ledger::store::Lease,
    object: &OwnedObject,
    from: &str,
    to: &str,
    basis: &hh_ledger::manifest::EventRef,
    ownerships: &OwnershipTable,
) -> Result<String, SpawnError> {
    if !ownerships.holds(from, object) {
        return Err(SpawnError::Refused(crate::types::SpawnRefused::NotOwned {
            object: object.to_json(),
        }));
    }
    let ev = crate::spawn::kernel_ev_pub(
        store,
        run_id,
        "control.ownership.transferred",
        Json::obj([
            ("object", object.to_json()),
            ("from", Json::str(from)),
            ("to", Json::str(to)),
            (
                "basis",
                Json::str(format!("{}:{}", basis.run_id, basis.event_id)),
            ),
        ]),
        vec![basis.clone()],
        None,
    )
    .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    let id = ev.event_id.clone();
    store
        .append(run_id, lease, vec![ev])
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    Ok(id)
}

/// `return_ownership(store, run_id, lease, object, from, to, basis)` —
/// the O-5 return row (`control.ownership.returned`): `to` is the
/// grantor the ownership restores to (child end / revocation escrow —
/// the table's fold treats it like a transfer).
pub fn return_ownership(
    store: &mut Store,
    run_id: &str,
    lease: &hh_ledger::store::Lease,
    object: &OwnedObject,
    from: &str,
    to: &str,
    basis: &hh_ledger::manifest::EventRef,
) -> Result<String, SpawnError> {
    let ev = crate::spawn::kernel_ev_pub(
        store,
        run_id,
        "control.ownership.returned",
        Json::obj([
            ("object", object.to_json()),
            ("from", Json::str(from)),
            ("to", Json::str(to)),
            (
                "basis",
                Json::str(format!("{}:{}", basis.run_id, basis.event_id)),
            ),
        ]),
        vec![basis.clone()],
        None,
    )
    .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    let id = ev.event_id.clone();
    store
        .append(run_id, lease, vec![ev])
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    Ok(id)
}

/// `release_resource_locks(store, run_id, lease, child_run_id)` — the
/// `resource:*` scoped leases a spawn held for `child_run_id` (holder
/// `subagent:<child>`) release at the child's terminal — the "slice
/// remainder releases" rule for coordination keys (crash matrix).
pub fn release_resource_locks(
    store: &mut Store,
    run_id: &str,
    lease: &hh_ledger::store::Lease,
    child_run_id: &str,
) -> Result<Vec<String>, SpawnError> {
    let holder = format!("subagent:{child_run_id}");
    let held = store
        .scoped_leases_for(run_id, &holder)
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    let mut released = Vec::new();
    for l in &held {
        store
            .lease_release(run_id, lease, l, "child_terminal")
            .map_err(|e| SpawnError::Kernel(format!("release resource lease: {e}")))?;
        released.push(l.scope.spelling());
    }
    Ok(released)
}
