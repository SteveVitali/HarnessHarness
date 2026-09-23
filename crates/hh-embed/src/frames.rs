//! The `EventFrame → Frame` adapter (§7.4 §5). One adapter per
//! subscription — it holds the per-stream state the frames need:
//! the ephemeral `order` counter, the class filter (`filter.classes`
//! allowlist ∧ the host's `opt_out_notifications` denylist), and the
//! open stream items (`model.call.requested` opens `item_started`;
//! `model.call.failed` aborts it).
//!
//! - `Durable` → `durable{seq, hash, event}` — the ledger's own words,
//!   filtered by class.
//! - `Sync{at_seq}` → `sync{from_seq, through_seq, head_hash}` — the
//!   catch-up range just delivered plus the head it is valid at.
//! - `Ephemeral` → `ephemeral{kind, scope, attempt, order, payload}` —
//!   `kind` from [`hh_embed_schema::ephemeral_kind_of`].
//! - `Lagged{missed_from_seq}` → `lagged{dropped:{ephemeral:n},
//!   resume_from_seq}` — `n` counts the ephemeral frames this adapter
//!   has emitted since the last lagged/durable boundary (best-effort
//!   accounting; durable rows are never dropped — the ledger replays
//!   them).
//! - `Closed{reason}` → `closed{run_ended|kernel_shutdown}`.
//! - `Rewind` → live at S2.9 (`navigate`/`rollback` land `head.moved`) —
//!   the adapter emits the public `rewind` frame (§5a.1 §4).
//!
//! `model.call.requested` durable frames also synthesize an
//! `item_started` frame *after* the durable one (anchor_seq = the
//! request's seq); `model.call.failed` closes it with `item_aborted`
//! (the response's durable row retires a successful item — no frame).

use hh_embed_schema::frames::{ephemeral_kind_of, Frame};
use hh_ledger::event::{CloseReason, EventFrame};
use hh_wire::json::Json;

/// Per-subscription adapter state.
#[derive(Debug, Default)]
pub struct FrameAdapter {
    /// Class allowlist (`stream_events.filter.classes`) — prefixes.
    allow: Vec<String>,
    /// The host's `opt_out_notifications` denylist — prefixes.
    deny: Vec<String>,
    /// The per-subscription ephemeral order counter.
    order: i64,
    /// Ephemeral frames emitted since the last lagged boundary —
    /// the `dropped{ephemeral:n}` accounting input (the ledger does
    /// not report a count; this adapter counts its own emissions).
    eph_emitted: i64,
    /// The seq the current replay started at (`sync.from_seq`).
    replay_from: i64,
    /// Open stream items — `model_call_id → attempt`.
    open_items: std::collections::BTreeMap<String, i64>,
}

impl FrameAdapter {
    pub fn new(allow: Option<Vec<String>>, deny: Vec<String>, replay_from: i64) -> Self {
        FrameAdapter {
            allow: allow.unwrap_or_default(),
            deny,
            order: 0,
            eph_emitted: 0,
            replay_from,
            open_items: std::collections::BTreeMap::new(),
        }
    }

    /// True when `class` survives the allow ∧ deny filter.
    fn admits(&self, class: &str) -> bool {
        if !self.allow.is_empty() && !self.allow.iter().any(|p| class.starts_with(p.as_str())) {
            return false;
        }
        !self.deny.iter().any(|p| class.starts_with(p.as_str()))
    }

    /// Map one ledger `EventFrame` to zero or more contract `Frame`s.
    pub fn map(&mut self, f: EventFrame) -> Vec<Frame> {
        match f {
            EventFrame::Durable { seq, event, hash } => {
                let mut out = Vec::new();
                let class = event.class.clone();
                if self.admits(&class) {
                    out.push(Frame::Durable {
                        seq: seq as i64,
                        hash,
                        event: event.to_json(),
                    });
                }
                // Stream-item lifecycle — the durable rows open/close it.
                if class == "model.call.requested" {
                    let mc = event
                        .scope
                        .model_call_id
                        .clone()
                        .unwrap_or_else(|| format!("mc@{seq}"));
                    self.open_items.insert(mc.clone(), 1);
                    out.push(Frame::ItemStarted {
                        item_id: mc,
                        attempt: 1,
                        anchor_seq: seq as i64,
                    });
                } else if class == "model.call.failed" {
                    let mc = event
                        .payload
                        .get("model_call_id")
                        .and_then(Json::as_str)
                        .map(String::from)
                        .or_else(|| event.scope.model_call_id.clone())
                        .unwrap_or_default();
                    if let Some(attempt) = self.open_items.remove(&mc) {
                        let reason = event
                            .payload
                            .get("error")
                            .and_then(|e| e.get("class"))
                            .and_then(Json::as_str)
                            .unwrap_or("unknown")
                            .to_string();
                        out.push(Frame::ItemAborted {
                            item_id: mc,
                            attempt,
                            reason,
                        });
                    }
                }
                out
            }
            EventFrame::Sync { at_seq } => vec![Frame::Sync {
                from_seq: self.replay_from,
                through_seq: at_seq as i64,
                head_hash: String::new(),
            }],
            EventFrame::Ephemeral { event } => {
                if !self.admits(&event.class) {
                    return Vec::new();
                }
                self.order += 1;
                self.eph_emitted += 1;
                vec![Frame::Ephemeral {
                    kind: ephemeral_kind_of(&event.class).to_string(),
                    scope: Json::obj([
                        ("run_id", Json::str(event.run_id.clone())),
                        ("event_id", Json::str(event.event_id.clone())),
                    ]),
                    attempt: 0,
                    order: self.order,
                    payload: event.payload.clone(),
                }]
            }
            EventFrame::Lagged { missed_from_seq } => {
                let dropped = self.eph_emitted;
                self.eph_emitted = 0;
                vec![Frame::Lagged {
                    dropped_ephemeral: dropped,
                    resume_from_seq: missed_from_seq as i64,
                }]
            }
            EventFrame::Closed { reason } => vec![Frame::Closed {
                reason: match reason {
                    CloseReason::RunFinished => "run_ended".to_string(),
                    CloseReason::StoreClosed => "kernel_shutdown".to_string(),
                },
                detail: None,
            }],
            // `Rewind` — the `head.moved` rebase signal (S2.9; §5a.1 §4 —
            // "a `head.moved` is delivered as a `rewind`"). The durable row
            // also streams; this frame is the consumer's cue to rebase to
            // `to_seq` (events past it are off the live branch — durable in
            // the log, never rewritten).
            EventFrame::Rewind {
                to_seq,
                to_event_id,
                reason,
            } => vec![Frame::Rewind {
                to_seq: to_seq as i64,
                to_event_id,
                reason,
            }],
        }
    }

    /// Patch the `sync` frame's `head_hash` — the adapter learns it
    /// lazily because `EventFrame::Sync` carries only the seq (the
    /// ledger's word, looked up by the caller from the head it saw at
    /// subscribe time).
    pub fn fix_sync_head(frames: &mut [Frame], head_hash: &str) {
        for f in frames.iter_mut() {
            if let Frame::Sync { head_hash: h, .. } = f {
                if h.is_empty() {
                    *h = head_hash.to_string();
                }
            }
        }
    }
}
