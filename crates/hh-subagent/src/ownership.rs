//! The ownership ledger (ADR-0191 O-1…O-6; `OwnershipRecord` is
//! `no-precedent` — this module is its Stage-4 realization).
//!
//! Ownership is **not** authority: a `control.ownership.transferred` row
//! moves an [`crate::types::OwnedObject`] between holders; authority still
//! gates every effect through the handle set. The projection folds the
//! parent run's `control.ownership.transferred` events plus the declared
//! roots (`manifest`-level seeds passed at construction — a run owns its
//! own workspace tree at open).
//!
//! R-2.11 (ADR-0343 D3): the pure half — [`OwnedObject`],
//! [`OwnershipTable`], [`WriteVerdict`] and [`check_write`] (monitor
//! check 8, §5e.5) — lives in `hh_monitor::ownership` so the seven-stage
//! dispatcher (`hh-env`, a *lower* tier than this crate) runs the check
//! at `prepare` without a dependency cycle. This module keeps the verb
//! half — the kernel-minted row producers and the lease-release sweep —
//! and re-exports the monitor's types so every existing call site keeps
//! its `hh_subagent::ownership::*` spelling (CC1: one implementation).

use hh_ledger::store::Store;
use hh_wire::json::Json;

use crate::types::OwnedObject;
use crate::types::SpawnError;

// The monitor half — re-exported so the public path is stable (the
// implementation moved down-tier at R-2.11; no second spelling).
pub use hh_monitor::ownership::{check_write, OwnershipTable, WriteVerdict};

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
#[allow(clippy::too_many_arguments)]
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
