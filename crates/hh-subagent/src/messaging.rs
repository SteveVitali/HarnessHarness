//! Tree-local, parent-mediated peer messaging (§5e.3; ADR-0191 D6–D8,
//! M-2; OQ-417 caps; OQ-428 defaults).
//!
//! One mechanism: `send_message` appends `control.message.sent` on the
//! *sender's* ledger (the row is the audit — `message_id`, `to`, `body_ref`,
//! the label `⊔(ctx(from), (delegate, ∅, Public))` — never an endorsement,
//! never ownership). Delivery is an occurrence on the receiver's
//! `peer_message{from}` subscription recorded under the *receiver's* lease
//! — the parent mediates reach and ordering (sibling→sibling relays through
//! the parent's `inbox_scan` pass at its decision point); a finished
//! receiver is `undeliverable` (audited, C-9's skipped-occurrence rule).
//! `broadcast` and `sibling` gates read the sealed `MessagingPolicy` —
//! OQ-428's ratified defaults are `false`; tree-local reach only.

use std::collections::BTreeMap;

use hh_ledger::manifest::EventRef;
use hh_ledger::store::{Lease, Store};
use hh_ledger::LedgerError;
use hh_wire::json::Json;

use crate::spawn::{fold_children, kernel_ev_pub};
use crate::types::SpawnError;
use crate::types::*;

/// `send_message(ctx) → Receipt` — the §5e.3 op. Refusals ledger
/// `control.message.refused{reason}` on the sender's run and return the
/// `refused` receipt; accepts ledger `control.message.sent` and return
/// `queued`.
pub struct SendCtx<'a> {
    /// The store.
    pub store: &'a mut Store,
    /// The sender run.
    pub sender_run_id: &'a str,
    /// The sender's writer lease.
    pub sender_lease: &'a Lease,
    /// The target run (`parent_run_id | sibling | direct child`).
    pub to: &'a str,
    /// The body bytes (offloaded into the blob pool — the row carries
    /// `body_ref`, never the text).
    pub body: &'a [u8],
    /// `follow_up` at this slice (`steer` is the later wakeup arm and is a
    /// typed `ModeUnsupported` refusal, never a silent downgrade).
    pub delivery_mode: hh_ledger::wakeup::DeliveryMode,
    /// The caller's idempotency key — a retry names the same
    /// `(sender, key)` and returns the existing `message_id`; a different
    /// argument under one key is a kernel-side conflict, never a second
    /// send.
    pub idempotency_key: &'a str,
    /// The sender's sealed `MessagingPolicy` (the direction gates).
    pub policy: &'a MessagingPolicy,
    /// The kernel caps (OQ-417 constants at this stage).
    pub caps: MessageCaps,
    /// The sender's relationship to the receiver: `parent | child |
    /// sibling` (the caller's tree view; the op never infers).
    pub direction: &'a str,
    /// The sender's provenance ctx label (`ctx(from)` the row stamps).
    pub ctx_label: &'a Json,
    /// The causing event (decision/turn row — `causes[]`).
    pub caused_by: Option<&'a EventRef>,
}

/// `send_message` — refusal-first (typed, ledgered), then the `sent` row.
pub fn send_message(ctx: &mut SendCtx) -> Result<Receipt, SpawnError> {
    if ctx.delivery_mode == hh_ledger::wakeup::DeliveryMode::Steer {
        return Err(SpawnError::Refused(SpawnRefused::ModeUnsupported {
            detail: "delivery_mode = steer is the later wakeup arm".into(),
        }));
    }
    let body_addr = hh_identity::idp::address(ctx.body, "text/plain");
    let body_ref = body_addr.id();
    // Idempotent retry — `(sender, idempotency_key)` returns the durable
    // send it already made. A key replayed with different arguments is a
    // kernel conflict (the caller changed the request), never a second
    // effect.
    for e in ctx
        .store
        .events(ctx.sender_run_id)
        .map_err(|e| SpawnError::Kernel(e.to_string()))?
    {
        if e.class != "control.message.sent" && e.class != "control.message.refused" {
            continue;
        }
        if e.payload.get("idempotency_key").and_then(Json::as_str) != Some(ctx.idempotency_key) {
            continue;
        }
        let same = e.payload.get("to").and_then(Json::as_str) == Some(ctx.to)
            && e.payload.get("direction").and_then(Json::as_str) == Some(ctx.direction)
            && e.payload.get("body_ref").and_then(Json::as_str) == Some(body_ref.as_str())
            && e.payload.get("delivery_mode").and_then(Json::as_str)
                == Some(ctx.delivery_mode.as_str());
        if !same {
            return Err(SpawnError::Kernel(format!(
                "idempotency key {} replayed with different message arguments",
                ctx.idempotency_key
            )));
        }
        let status = if e.class == "control.message.sent" {
            MessageStatus::Queued
        } else {
            MessageStatus::Refused(message_refused_from_str(
                e.payload
                    .get("reason")
                    .and_then(Json::as_str)
                    .unwrap_or("policy"),
                e.payload
                    .get("direction")
                    .and_then(Json::as_str)
                    .unwrap_or(ctx.direction),
                e.payload
                    .get("bytes")
                    .and_then(Json::as_int)
                    .map(|v| v.max(0) as u64)
                    .unwrap_or(ctx.body.len() as u64),
            ))
        };
        return Ok(Receipt {
            message_id: e.event_id.clone(),
            status,
        });
    }
    // ── reach: tree-local only ───────────────────────────────────────────
    let allowed = match ctx.direction {
        "child" => ctx.policy.parent_to_child,  // parent → child
        "parent" => ctx.policy.child_to_parent, // child → parent
        "sibling" => ctx.policy.sibling,        // relayed via parent
        "broadcast" => ctx.policy.broadcast,
        _ => false,
    };
    if !allowed {
        return refuse(
            ctx,
            MessageRefused::Policy {
                direction: ctx.direction.to_string(),
            },
        );
    }
    // ── size cap ─────────────────────────────────────────────────────────
    if ctx.body.len() as u64 > ctx.caps.max_body_bytes {
        return refuse(
            ctx,
            MessageRefused::Size {
                bytes: ctx.body.len() as u64,
            },
        );
    }
    // ── rate cap (sent-count vs `max_per_sender`) ────────────────────────
    let sent_count = ctx
        .store
        .events(ctx.sender_run_id)
        .map_err(|e| SpawnError::Kernel(e.to_string()))?
        .iter()
        .filter(|e| e.class == "control.message.sent")
        .count() as u64;
    if sent_count >= ctx.caps.max_per_sender {
        return refuse(ctx, MessageRefused::Rate);
    }
    // ── queue cap (receiver's pending vs `min(policy, hard)`) ───────────
    let pending = pending_count(ctx.store, ctx.to)?;
    let bound = ctx.policy.max_pending.min(ctx.caps.max_pending_hard);
    if pending >= bound {
        return refuse(ctx, MessageRefused::Queue);
    }
    // ── sent ─────────────────────────────────────────────────────────────
    // The body lands in the blob pool before the audit row cites it — a
    // durable `body_ref` never dangles; an aborted append can only leave an
    // unreferenced blob (CC3 prefers orphan bytes to a missing cited body).
    let body_addr = ctx
        .store
        .put_blob(ctx.body, "text/plain")
        .map_err(|e| SpawnError::Kernel(format!("message body blob: {e}")))?;
    let body_ref = body_addr.id();
    let causes: Vec<EventRef> = ctx.caused_by.cloned().into_iter().collect();
    let ev = kernel_ev_pub(
        ctx.store,
        ctx.sender_run_id,
        "control.message.sent",
        Json::obj([
            ("to", Json::str(ctx.to)),
            ("body_ref", Json::str(body_ref.clone())),
            ("direction", Json::str(ctx.direction)),
            ("delivery_mode", Json::str(ctx.delivery_mode.as_str())),
            ("idempotency_key", Json::str(ctx.idempotency_key)),
            (
                "label",
                Json::obj([
                    ("ctx", ctx.ctx_label.clone()),
                    ("floor", Json::str("(delegate, ∅, Public)")),
                ]),
            ),
        ]),
        causes,
        None,
    )
    .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    let message_id = ev.event_id.clone();
    ctx.store
        .append(ctx.sender_run_id, ctx.sender_lease, vec![ev])
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    Ok(Receipt {
        message_id,
        status: MessageStatus::Queued,
    })
}

/// A `control.message.refused` row + the `refused` receipt.
fn refuse(ctx: &mut SendCtx, r: MessageRefused) -> Result<Receipt, SpawnError> {
    let ev = kernel_ev_pub(
        ctx.store,
        ctx.sender_run_id,
        "control.message.refused",
        Json::obj([
            ("to", Json::str(ctx.to)),
            ("reason", Json::str(r.as_str())),
            ("direction", Json::str(ctx.direction)),
            ("delivery_mode", Json::str(ctx.delivery_mode.as_str())),
            ("idempotency_key", Json::str(ctx.idempotency_key)),
            (
                "body_ref",
                Json::str(hh_identity::idp::address(ctx.body, "text/plain").id()),
            ),
            (
                "bytes",
                Json::Int(ctx.body.len().min(i64::MAX as usize) as i64),
            ),
        ]),
        vec![],
        None,
    )
    .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    let message_id = ev.event_id.clone();
    ctx.store
        .append(ctx.sender_run_id, ctx.sender_lease, vec![ev])
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    Ok(Receipt {
        message_id,
        status: MessageStatus::Refused(r),
    })
}

/// The receiver's pending-message count — `sent` occurrences recorded on
/// its `peer_message` subscriptions that have not fired (`deliver` is the
/// drain; the queue bound is the policy's).
fn pending_count(store: &Store, receiver_run_id: &str) -> Result<u64, SpawnError> {
    let events = store
        .events(receiver_run_id)
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    let mut occurred = 0u64;
    let mut fired = 0u64;
    for e in events {
        match e.class.as_str() {
            "control.wakeup.occurred" => {
                if e.payload
                    .get("occurrence_key")
                    .and_then(Json::as_str)
                    .is_some_and(|k| k.starts_with("message:"))
                {
                    occurred += 1;
                }
            }
            "control.wakeup.fired" | "control.wakeup.skipped" => {
                if e.payload
                    .get("occurrence_key")
                    .and_then(Json::as_str)
                    .is_some_and(|k| k.starts_with("message:"))
                {
                    fired += 1;
                }
            }
            _ => {}
        }
    }
    Ok(occurred.saturating_sub(fired))
}

/// The parent-mediated delivery pass — `inbox_scan(store, receiver_run,
/// receiver_lease, run_ids)` folds `control.message.sent` rows across the
/// tree addressed to `receiver_run` and records a `message:<id>` occurrence
/// on the receiver's `peer_message{from}` subscription per sender (the
/// receiver's own lease writes the row — a receiver without a live lease
/// is `undeliverable`, and a `finished` receiver's append is
/// `skipped{run_finished}` — audited). Duplicates dedupe by occurrence
/// key (`skipped{duplicate_occurrence}` — never a second fire).
///
/// Returns the message ids this pass delivered (occurrence-durable; the
/// receiver's `deliver_wakeup` fires them under W-3).
pub fn inbox_scan(
    store: &mut Store,
    receiver_run_id: &str,
    receiver_lease: &Lease,
    tree_run_ids: &[String],
) -> Result<Vec<String>, SpawnError> {
    // Map sender → peer_message subscription id on the receiver.
    let mut subs: BTreeMap<String, String> = BTreeMap::new();
    for e in store
        .events(receiver_run_id)
        .map_err(|e| SpawnError::Kernel(e.to_string()))?
    {
        if e.class != "control.wakeup.scheduled" {
            continue;
        }
        let sub = e.payload.get("subscription");
        let trig = sub.and_then(|s| s.get("trigger"));
        if trig.and_then(|t| t.get("type")).and_then(Json::as_str) == Some("peer_message") {
            if let (Some(from), Some(id)) = (
                trig.and_then(|t| t.get("from")).and_then(Json::as_str),
                sub.and_then(|s| s.get("subscription_id"))
                    .and_then(Json::as_str),
            ) {
                subs.insert(from.to_string(), id.to_string());
            }
        }
    }
    let mut delivered = Vec::new();
    for run_id in tree_run_ids {
        if run_id == receiver_run_id {
            continue;
        }
        let events = match store.events(run_id) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let sent: Vec<(String, String, String)> = events
            .iter()
            .filter(|e| {
                e.class == "control.message.sent"
                    && e.payload.get("to").and_then(Json::as_str) == Some(receiver_run_id)
            })
            .map(|e| {
                (
                    run_id.clone(),
                    e.event_id.clone(),
                    e.payload
                        .get("body_ref")
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_string(),
                )
            })
            .collect();
        for (from, message_id, body_ref) in sent {
            let sub_id = match subs.get(&from) {
                Some(s) => s.clone(),
                None => {
                    // No `peer_message{from}` subscription — lazily create
                    // it under the receiver's lease (the subscription is
                    // the receiver's own declared interest).
                    store
                        .wakeup_subscribe(
                            receiver_run_id,
                            receiver_lease,
                            hh_ledger::wakeup::Trigger::PeerMessage { from: from.clone() },
                            hh_ledger::wakeup::WakeupPolicy::default_policy(),
                            &EventRef {
                                // The creating fact is the sender-side
                                // `control.message.sent` row, not a row on
                                // the receiver's log.
                                run_id: from.clone(),
                                event_id: message_id.clone(),
                            },
                        )
                        .map_err(|e| SpawnError::Kernel(e.to_string()))?
                }
            };
            match store.wakeup_occurred(
                receiver_run_id,
                receiver_lease,
                &sub_id,
                &format!("message:{message_id}"),
                Some(if body_ref.is_empty() {
                    format!("{from}:{message_id}")
                } else {
                    body_ref.clone()
                })
                .as_deref(),
                store.now_ms(),
            ) {
                Ok(hh_ledger::wakeup::OccurOutcome::Occurred(_)) => delivered.push(message_id),
                Ok(hh_ledger::wakeup::OccurOutcome::Skipped(_)) => {
                    // `duplicate_occurrence` — already delivered; the
                    // audited skip row stands, the id is not re-reported.
                }
                Err(LedgerError::RunFinished { .. }) => {
                    // Audited undeliverable — the receiver is finished; the
                    // sent row stands and no occurrence can land (C-9).
                }
                Err(e) => return Err(SpawnError::Kernel(e.to_string())),
            }
        }
    }
    Ok(delivered)
}

fn message_refused_from_str(reason: &str, direction: &str, bytes: u64) -> MessageRefused {
    match reason {
        "size" => MessageRefused::Size { bytes },
        "rate" => MessageRefused::Rate,
        "queue" => MessageRefused::Queue,
        "reach" => MessageRefused::Reach {
            target: direction.to_string(),
        },
        _ => MessageRefused::Policy {
            direction: direction.to_string(),
        },
    }
}

/// The receiver-side lazy `peer_message` subscription — a run declares
/// `peer_message{from}` per peer (the parent's mediation creates it under
/// the *receiver's* lease on first delivery; a run may also declare it
/// up-front).
pub fn declare_peer_inbox(
    store: &mut Store,
    run_id: &str,
    lease: &Lease,
    from: &str,
    created_by: &EventRef,
) -> Result<String, SpawnError> {
    store
        .wakeup_subscribe(
            run_id,
            lease,
            hh_ledger::wakeup::Trigger::PeerMessage {
                from: from.to_string(),
            },
            hh_ledger::wakeup::WakeupPolicy::default_policy(),
            created_by,
        )
        .map_err(|e| SpawnError::Kernel(e.to_string()))
}

/// The sibling set a parent relays between (tree-local reach: the parent's
/// direct children; the parent's own run id is always reachable).
pub fn relay_set(store: &Store, parent_run_id: &str) -> Result<Vec<String>, SpawnError> {
    let mut set: Vec<String> = fold_children(store, parent_run_id)?
        .into_iter()
        .map(|(c, _)| c)
        .collect();
    set.push(parent_run_id.to_string());
    Ok(set)
}
