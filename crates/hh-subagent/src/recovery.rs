//! Parent-side subagent recovery (§5a.3 recovery-table `subagent` row;
//! C-3/C-4; the KP-20/KP-21 adopt paths).
//!
//! On a parent's `restore` (or at its next decision point after a crash):
//!
//! * **`spawned` rows with no child run** — the spawn died between the
//!   spawned append and `open_run` (the KP-21 late half). The named
//!   `child_run_id` can never exist; the child is
//!   `cancelled{infrastructure_failure{spawn_interrupted}}` so the
//!   ADR-0066 obligation "every `spawned` has a `created` and a terminal"
//!   holds and `fan_out` decrements.
//! * **children whose `child_terminal` subscription never landed** (crash
//!   between `open_run` and `subscribe`) — re-subscribed (the subscription
//!   is the durable wakeup; re-subscribing is idempotent only if the first
//!   never landed — `fold_children` proves it).
//! * **finished children with no terminal row** — `drain_child_terminal`
//!   completes them (`result`/`cancelled{infrastructure_failure}`).
//! * **`attendance`/`awaiting_child` suspensions** — the parent's own
//!   `lifecycle.run.suspended` fold already names its subscriptions;
//!   this module only guarantees the *subscription rows* exist so a
//!   resumed parent's `deliver_wakeup` can fire `child_terminal`.

use hh_ledger::manifest::EventRef;
use hh_ledger::store::{Lease, Store};
use hh_wire::json::Json;

use crate::result::drain_child_terminal;
use crate::spawn::{fold_children, kernel_ev_pub};
use crate::types::SpawnError;
use crate::types::SubagentSpec;

/// What a parent restore reported (audited on the parent's ledger).
#[derive(Debug, Default)]
pub struct RecoveryReport {
    /// `spawned` children adopted as `cancelled{spawn_interrupted}`.
    pub interrupted: Vec<String>,
    /// `child_terminal` subscriptions re-created.
    pub resubscribed: Vec<String>,
    /// Terminal children completed by the drain.
    pub completed: Vec<String>,
}

/// `recover_parent(store, parent_run_id, lease)` — the subagent half of a
/// parent restore. Idempotent (every step re-reads the fold; a second pass
/// is a no-op — KP-20).
pub fn recover_parent(
    store: &mut Store,
    parent_run_id: &str,
    parent_lease: &Lease,
) -> Result<RecoveryReport, SpawnError> {
    let mut report = RecoveryReport::default();
    let children = fold_children(store, parent_run_id)?;

    // Subscription ids already present (the scheduled rows fold).
    let mut have_sub = std::collections::BTreeSet::new();
    for e in store
        .events(parent_run_id)
        .map_err(|e| SpawnError::Kernel(e.to_string()))?
    {
        if e.class != "control.wakeup.scheduled" {
            continue;
        }
        let trig = e.payload.get("subscription").and_then(|s| s.get("trigger"));
        if let Some(c) = trig
            .and_then(|t| t.get("child_run_id"))
            .and_then(Json::as_str)
        {
            have_sub.insert(c.to_string());
        }
    }

    for (child_run_id, rec) in &children {
        // Interrupted spawn — spawned but the run never opened.
        if !store.has_run(child_run_id) {
            if rec.terminal_class.is_none() {
                let mut ev = kernel_ev_pub(
                    store,
                    parent_run_id,
                    "control.subagent.cancelled",
                    Json::obj([
                        ("child_run_id", Json::str(child_run_id.as_str())),
                        ("reason", Json::str("infrastructure_failure")),
                        ("failure_kind", Json::str("spawn_interrupted")),
                        (
                            "delegation_ref",
                            rec.spawned
                                .get("delegation_ref")
                                .cloned()
                                .unwrap_or(Json::Null),
                        ),
                    ]),
                    vec![],
                    None,
                )
                .map_err(|e| SpawnError::Kernel(e.to_string()))?;
                ev.scope.child_run_id = Some(child_run_id.clone());
                store
                    .append(parent_run_id, parent_lease, vec![ev])
                    .map_err(|e| SpawnError::Kernel(e.to_string()))?;
                // The interrupted spawn's `spawns` reservation dies with
                // the terminal row — recovery must leave nothing held
                // (KP-21), not merely mark the edge cancelled.
                if let Some(reservation_id) =
                    rec.spawned.get("reservation_id").and_then(Json::as_str)
                {
                    if let Ok(mut account) = hh_budget::account::Account::open(store, parent_run_id)
                    {
                        let _ = account.release(parent_lease, reservation_id);
                    }
                }
                report.interrupted.push(child_run_id.clone());
            }
            continue;
        }
        // Live child without its `child_terminal` subscription — crash
        // between `open_run` and `subscribe`.
        if !have_sub.contains(child_run_id) {
            let created_by = EventRef {
                run_id: parent_run_id.to_string(),
                event_id: rec.spawn_event_id.clone(),
            };
            store
                .wakeup_subscribe(
                    parent_run_id,
                    parent_lease,
                    hh_ledger::wakeup::Trigger::ChildTerminal {
                        child_run_id: child_run_id.clone(),
                    },
                    hh_ledger::wakeup::WakeupPolicy::default_policy(),
                    &created_by,
                )
                .map_err(|e| SpawnError::Kernel(e.to_string()))?;
            report.resubscribed.push(child_run_id.clone());
        }
    }

    // Terminal drain — finished children whose result row never landed
    // (parent died between the child's terminal and the parent's cue
    // handling; C-4).
    report.completed = drain_child_terminal(
        store,
        parent_run_id,
        parent_lease,
        &std::collections::BTreeMap::new(),
    )?;
    Ok(report)
}

/// `cancel_on_revocation` — C-7's cascade: when a parent handle revokes,
/// every live child whose handle set descends from it is
/// `cancelled{revoked}` (the child's own covered proposals already refuse
/// `NoCoveringGrant` — the typed row is the parent's record of the
/// cascade). The child's own runtime sees the revocation through its
/// handle validity; this row is the *parent's* bookkeeping half.
pub fn cancel_on_revocation(
    store: &mut Store,
    parent_run_id: &str,
    parent_lease: &Lease,
    revoked_parent_handle: &str,
) -> Result<Vec<String>, SpawnError> {
    let mut done = Vec::new();
    for (child_run_id, rec) in fold_children(store, parent_run_id)? {
        if rec.terminal_class.is_some() {
            continue;
        }
        let parent_handle = rec
            .spawned
            .get("parent_handle")
            .and_then(Json::as_str)
            .unwrap_or_default();
        if parent_handle != revoked_parent_handle {
            continue;
        }
        let spec = store
            .manifest(&child_run_id)
            .ok()
            .and_then(|m| m.extra.get("subagent_spec"))
            .and_then(SubagentSpec::from_json);
        let budget_id = rec
            .spawned
            .get("budget_id")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        crate::result::record_child_cancelled(
            store,
            parent_run_id,
            parent_lease,
            &child_run_id,
            &spec.unwrap_or_else(crate::result::default_spec),
            crate::types::CancelReason::Revoked,
            &budget_id,
        )?;
        done.push(child_run_id);
    }
    Ok(done)
}

/// The deadline-expiry surface — `TimeoutPolicy[subagent]` names the bound;
/// `cancel_unresponsive` is the row layer (re-exported for one call site).
pub use crate::result::cancel_unresponsive as deadline_walk;
