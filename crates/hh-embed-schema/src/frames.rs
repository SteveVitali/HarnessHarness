//! The `hh-embed/1` frame model (§7.4 §5; ADR-0176 D7). `stream_events`
//! yields `Frame`s — a tagged sum, the only notification shape.
//!
//! ```text
//! Frame = durable      {seq, hash, event}
//!       | sync         {from_seq, through_seq, head_hash}
//!       | ephemeral    {kind, scope, attempt, order, payload}
//!       | item_started {item_id, attempt, anchor_seq}
//!       | item_aborted {item_id, attempt, reason}
//!       | lagged       {dropped, resume_from_seq}
//!       | closed       {reason, detail?}
//! ```
//!
//! Every frame carries `durability` — `ledger` on `durable`, `ephemeral`
//! on everything else (I3's surface). A `durable` frame's `hash` is the
//! ledger hash of `seq` — never a recomputation, always the ledger's own
//! word. `sync` reports the catch-up range replay just completed and the
//! head it is valid at; `item_started` opens a stream item (`attempt`,
//! `anchor_seq`) retired by a durable row or `item_aborted`; `lagged`
//! carries bounded-channel accounting (`dropped{ephemeral:n}` — durable
//! replay never drops) plus the transport fact `resume_from_seq` a
//! re-issued `stream_events` resumes from (a binding may add transport
//! facts only — §7.4 §5).
//!
//! A slow consumer receives `lagged` and, past the kernel's durable-
//! backlog bound, `closed{slow_consumer}` — the subscription is dead and
//! the client re-subscribes from its last seen seq (§7.4 §5.1).

use crate::errors::EmbedError;
use crate::strict::{closed_str, StrictObj};
use hh_wire::json::Json;
use std::collections::BTreeMap;

/// A stream frame — the tagged sum (§7.4 §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// A ledger event, durability `ledger` — `{seq, hash, event}`.
    Durable {
        seq: i64,
        /// The ledger hash of `seq` — `event_hashes[seq]` verbatim.
        hash: String,
        /// The canonical `Event` record.
        event: Json,
    },
    /// Replay caught up to live — `{from_seq, through_seq, head_hash}`:
    /// the inclusive durable range the replay just delivered and the
    /// head hash it is valid at.
    Sync {
        from_seq: i64,
        through_seq: i64,
        head_hash: String,
    },
    /// A kernel-emitted ephemeral frame (E3: never durable, never in
    /// `read`) — `{kind, scope, attempt, order, payload}`. `order` is a
    /// per-subscription sequence the consumer uses to detect ephemeral
    /// loss that precedes a `lagged`.
    Ephemeral {
        kind: String,
        scope: Json,
        attempt: i64,
        order: i64,
        payload: Json,
    },
    /// A stream item opened — `{item_id, attempt, anchor_seq}`; retired
    /// by the item's durable rows or `item_aborted`.
    ItemStarted {
        item_id: String,
        attempt: i64,
        anchor_seq: i64,
    },
    /// A stream item aborted — `{item_id, attempt, reason}`.
    ItemAborted {
        item_id: String,
        attempt: i64,
        reason: String,
    },
    /// The consumer fell behind — `{dropped:{ephemeral:n},
    /// resume_from_seq}`. Durable events are never dropped (the
    /// consumer re-reads them via `read`/`stream_events` from
    /// `resume_from_seq`); the `dropped` map is ephemeral accounting.
    Lagged {
        dropped_ephemeral: i64,
        resume_from_seq: i64,
    },
    /// Terminal — `{reason, detail?}`; the subscription is dead.
    Closed {
        reason: String,
        detail: Option<String>,
    },
}

/// `closed` reasons (closed sum; `slow_consumer` is the AC-R-2.11.4-6
/// signal).
pub const CLOSED_REASONS: &[&str] = &[
    "complete",
    "cancelled",
    "slow_consumer",
    "run_ended",
    "session_detached",
    "kernel_shutdown",
];

/// `ephemeral.kind` closed sum — the surface kinds a kernel may emit
/// (§7.4 §5). `permission_rendering` carries a permission ask's
/// rendering; `delta`/`progress` are model-item surfaces; `guard_notice`
/// a guard's advisory; `log` anything else.
pub const EPHEMERAL_KINDS: &[&str] = &[
    "delta",
    "progress",
    "permission_rendering",
    "guard_notice",
    "log",
];

/// Map a ledger ephemeral class to the closed `ephemeral.kind` —
/// `security.permission.requested` is the permission channel's
/// rendering; everything else is a `log` (conservative default — a
/// class's kind may narrow later without a dialect bump, never widen
/// past the sum).
pub fn ephemeral_kind_of(class: &str) -> &'static str {
    match class {
        "security.permission.requested" => "permission_rendering",
        "control.guard.fired" | "control.guard.notice" => "guard_notice",
        _ => "log",
    }
}

impl Frame {
    /// `durability` for the frame — `ledger` for `durable`, `ephemeral`
    /// for everything else (sync/ephemeral/item/lagged/closed are
    /// stream-level).
    pub fn durability(&self) -> &'static str {
        match self {
            Frame::Durable { .. } => "ledger",
            _ => "ephemeral",
        }
    }

    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        match self {
            Frame::Durable { seq, hash, event } => {
                m.insert("kind".into(), Json::str("durable"));
                m.insert("seq".into(), Json::Int(*seq));
                m.insert("hash".into(), Json::str(hash.clone()));
                m.insert("event".into(), event.clone());
            }
            Frame::Sync {
                from_seq,
                through_seq,
                head_hash,
            } => {
                m.insert("kind".into(), Json::str("sync"));
                m.insert("from_seq".into(), Json::Int(*from_seq));
                m.insert("through_seq".into(), Json::Int(*through_seq));
                m.insert("head_hash".into(), Json::str(head_hash.clone()));
            }
            Frame::Ephemeral {
                kind,
                scope,
                attempt,
                order,
                payload,
            } => {
                m.insert("kind".into(), Json::str("ephemeral"));
                m.insert("ephemeral_kind".into(), Json::str(kind.clone()));
                m.insert("scope".into(), scope.clone());
                m.insert("attempt".into(), Json::Int(*attempt));
                m.insert("order".into(), Json::Int(*order));
                m.insert("payload".into(), payload.clone());
            }
            Frame::ItemStarted {
                item_id,
                attempt,
                anchor_seq,
            } => {
                m.insert("kind".into(), Json::str("item_started"));
                m.insert("item_id".into(), Json::str(item_id.clone()));
                m.insert("attempt".into(), Json::Int(*attempt));
                m.insert("anchor_seq".into(), Json::Int(*anchor_seq));
            }
            Frame::ItemAborted {
                item_id,
                attempt,
                reason,
            } => {
                m.insert("kind".into(), Json::str("item_aborted"));
                m.insert("item_id".into(), Json::str(item_id.clone()));
                m.insert("attempt".into(), Json::Int(*attempt));
                m.insert("reason".into(), Json::str(reason.clone()));
            }
            Frame::Lagged {
                dropped_ephemeral,
                resume_from_seq,
            } => {
                m.insert("kind".into(), Json::str("lagged"));
                m.insert(
                    "dropped".into(),
                    Json::obj([("ephemeral", Json::Int(*dropped_ephemeral))]),
                );
                m.insert("resume_from_seq".into(), Json::Int(*resume_from_seq));
            }
            Frame::Closed { reason, detail } => {
                m.insert("kind".into(), Json::str("closed"));
                m.insert("reason".into(), Json::str(reason.clone()));
                if let Some(d) = detail {
                    m.insert("detail".into(), Json::str(d.clone()));
                }
            }
        }
        m.insert("durability".into(), Json::str(self.durability()));
        Json::Obj(m)
    }

    /// Strict decode — `UnknownField{path}` on undeclared members,
    /// `SchemaViolation` on unknown `kind`/`reason`/`ephemeral_kind` (CC8).
    pub fn from_json(v: &Json, path: &str) -> Result<Self, EmbedError> {
        let mut s = StrictObj::new(v, path)?;
        let kind = closed_str(
            s.req("kind")?,
            &[
                "durable",
                "sync",
                "ephemeral",
                "item_started",
                "item_aborted",
                "lagged",
                "closed",
            ],
            &format!("{path}/kind"),
        )?;
        // `durability` is redundant-but-required on every frame (I3
        // surface); consume it and check it agrees with `kind`.
        let durability = s.req_str("durability")?;
        let expected = if kind == "durable" {
            "ledger"
        } else {
            "ephemeral"
        };
        if durability != expected {
            return Err(EmbedError::SchemaViolation {
                path: format!("{path}/durability"),
                code: "durability_mismatch".to_string(),
            });
        }
        let out = match kind.as_str() {
            "durable" => Frame::Durable {
                seq: s.req_int("seq")?,
                hash: s.req_str("hash")?,
                event: s.req("event")?.clone(),
            },
            "sync" => Frame::Sync {
                from_seq: s.req_int("from_seq")?,
                through_seq: s.req_int("through_seq")?,
                head_hash: s.req_str("head_hash")?,
            },
            "ephemeral" => {
                let eph_kind = closed_str(
                    s.req("ephemeral_kind")?,
                    EPHEMERAL_KINDS,
                    &format!("{path}/ephemeral_kind"),
                )?;
                Frame::Ephemeral {
                    kind: eph_kind,
                    scope: s.req("scope")?.clone(),
                    attempt: s.req_int("attempt")?,
                    order: s.req_int("order")?,
                    payload: s.req("payload")?.clone(),
                }
            }
            "item_started" => Frame::ItemStarted {
                item_id: s.req_str("item_id")?,
                attempt: s.req_int("attempt")?,
                anchor_seq: s.req_int("anchor_seq")?,
            },
            "item_aborted" => Frame::ItemAborted {
                item_id: s.req_str("item_id")?,
                attempt: s.req_int("attempt")?,
                reason: s.req_str("reason")?,
            },
            "lagged" => {
                let dropped = s.req("dropped")?;
                let dropped_ephemeral = match dropped {
                    Json::Obj(dm) => dm.get("ephemeral").and_then(Json::as_int).unwrap_or(0),
                    _ => 0,
                };
                Frame::Lagged {
                    dropped_ephemeral,
                    resume_from_seq: s.req_int("resume_from_seq")?,
                }
            }
            _ => {
                let reason =
                    closed_str(s.req("reason")?, CLOSED_REASONS, &format!("{path}/reason"))?;
                let detail = s.opt_str("detail")?;
                Frame::Closed { reason, detail }
            }
        };
        s.finish()?;
        Ok(out)
    }
}

/// The `stream.frame` notification params — `{subscription_id, frame}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamNotification {
    pub subscription_id: String,
    pub frame: Frame,
}

impl StreamNotification {
    /// The notification method name.
    pub const METHOD: &'static str = "stream.frame";

    pub fn to_json(&self) -> Json {
        Json::obj([
            ("subscription_id", Json::str(self.subscription_id.clone())),
            ("frame", self.frame.to_json()),
        ])
    }

    /// Build the full JSON-RPC notification envelope.
    pub fn to_notification(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("jsonrpc".into(), Json::str("2.0"));
        m.insert("method".into(), Json::str(Self::METHOD));
        m.insert("params".into(), self.to_json());
        Json::Obj(m)
    }
}
