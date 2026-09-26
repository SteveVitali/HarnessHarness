//! `subscribe(filter)` — the results store's change journal (§6.5 §2.2;
//! ADR-0161 D4; ADR-0294 D2): a `Stream` of
//! `RowHeadChanged | AnnotationChanged | SnapshotPublished` rows emitted
//! **only after the underlying write is durable** (every emission happens
//! after `write_atomic` returns — the store's durability boundary is the
//! file rename; nothing is emitted speculatively and ordering is the
//! commit order).
//!
//! The journal is a broadcast: every subscriber sees every event its
//! `kinds` filter admits, in emission order, from subscribe time forward
//! (replay of history is the store's own reads — `query_rows`,
//! `row_history`, `snapshots` — the journal reports *changes*, never
//! reconstructs state).

use std::collections::BTreeSet;
use std::sync::mpsc::{Receiver, Sender};

use hh_wire::json::Json;

/// The journal event kinds (the §6.5 §2.2 stream member vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum JournalKind {
    /// A row head moved (`record` committed a new head version).
    RowHeadChanged,
    /// The annotation index changed (`annotate` committed).
    AnnotationChanged,
    /// A leaderboard snapshot was published (`publish` committed).
    SnapshotPublished,
}

impl JournalKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            JournalKind::RowHeadChanged => "row_head_changed",
            JournalKind::AnnotationChanged => "annotation_changed",
            JournalKind::SnapshotPublished => "snapshot_published",
        }
    }

    /// Parse; `None` on any other spelling.
    pub fn parse(s: &str) -> Option<JournalKind> {
        match s {
            "row_head_changed" => Some(JournalKind::RowHeadChanged),
            "annotation_changed" => Some(JournalKind::AnnotationChanged),
            "snapshot_published" => Some(JournalKind::SnapshotPublished),
            _ => None,
        }
    }
}

/// One journal row — `{kind, subject, detail}`. `subject` is the member
/// the event is about (the row key, the annotated key, or the published
/// snapshot id); `detail` carries the new head version / definition ref.
#[derive(Debug, Clone, PartialEq)]
pub struct JournalEvent {
    /// The event kind.
    pub kind: JournalKind,
    /// The subject (row key id / snapshot id / annotation key id).
    pub subject: String,
    /// The detail (head version id, definition ref, …) — data, never a
    /// free-text message.
    pub detail: String,
}

impl JournalEvent {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("kind", Json::str(self.kind.as_str())),
            ("subject", Json::str(&self.subject)),
            ("detail", Json::str(&self.detail)),
        ])
    }
}

/// The subscription filter — `kinds` `None` admits every kind.
#[derive(Debug, Clone, Default)]
pub struct JournalFilter {
    /// The admitted kinds (`None` = all).
    pub kinds: Option<BTreeSet<JournalKind>>,
}

impl JournalFilter {
    /// Whether the event passes the filter.
    pub fn admits(&self, e: &JournalEvent) -> bool {
        match &self.kinds {
            None => true,
            Some(k) => k.contains(&e.kind),
        }
    }
}

/// A live journal subscription — `recv()`/`try_recv()` read committed
/// events in order.
pub struct JournalSubscription {
    rx: Receiver<JournalEvent>,
}

impl JournalSubscription {
    /// Block for the next event (`None` when every sender dropped).
    pub fn recv(&self) -> Option<JournalEvent> {
        self.rx.recv().ok()
    }

    /// The next committed event, if any.
    pub fn try_recv(&self) -> Option<JournalEvent> {
        self.rx.try_recv().ok()
    }
}

/// The store-side broadcast hub (interior mutability — `record`/`annotate`/
/// `publish` take `&self`).
#[derive(Debug, Default)]
pub struct JournalHub {
    senders: std::sync::Mutex<Vec<(JournalFilter, Sender<JournalEvent>)>>,
}

impl JournalHub {
    /// `subscribe(filter)` — every admitted event committed *after* the
    /// subscribe call arrives in order.
    pub fn subscribe(&self, filter: JournalFilter) -> JournalSubscription {
        let (tx, rx) = std::sync::mpsc::channel();
        self.senders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((filter, tx));
        JournalSubscription { rx }
    }

    /// Emit a committed event — the caller runs this *after* the durable
    /// write returns (post-commit emission is the whole contract; the
    /// call sites are placed accordingly). Delivery is best-effort: a
    /// dropped receiver is pruned; the filter is applied at emit so
    /// nothing inadmissible crosses the channel.
    pub(crate) fn emit(&self, e: JournalEvent) {
        let mut guard = self.senders.lock().unwrap_or_else(|err| err.into_inner());
        guard.retain(|(f, tx)| !f.admits(&e) || tx.send(e.clone()).is_ok());
    }
}
