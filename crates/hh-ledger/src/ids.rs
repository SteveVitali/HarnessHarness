//! The allocated-id model (ADR-0027 §1–§2): opaque, unique, **creation-time-sortable**
//! ids, minted under the one identity scheme (`idp/1` domain separation lives in
//! `hh-identity`; allocated ids are `{kind}-{ms}-{counter}-{tag}` strings — sortable by
//! construction, never content addresses).
//!
//! - `RunId` — allocated by `open_run`; globally unique via the store tag.
//! - `event_id` — supplied by the caller (the writer allocates them; `append` validates
//!   uniqueness), same `evt-…` shape.
//! - `effect_id` — **derived**, never allocated: `f(run_id, model_call_id, tool_call_id,
//!   ordinal)` so a re-parsed response can never duplicate intent (ADR-0027 §2).
//! - `lease_id`, `turn_id`, `model_call_id`, `tool_call_id` — allocated.
//!
//! Determinism: [`IdSource`] is injectable. The golden corpus and every test that pins
//! bytes drive [`SeqIds`]; production drives [`TimeIds`] over a [`Clock`].

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use hh_identity::idp::idp_id;
use hh_wire::json::Json;

/// The reserved root sentinel for `parent_event_id` (§5a.1 §3: "the root sentinel").
pub const ROOT_EVENT: &str = "root";

/// The `prev_hash` of seq 0 for a fresh run — a forked/continued run instead anchors to
/// the source head hash (ADR-0029 §2; ADR-0131 §5).
pub const GENESIS_HASH: &str = "genesis";

/// Wall-clock milliseconds — the ordering source for allocated ids and lease expiry.
/// This is *id material*, never a ledger fact (provenance `created_at` is logical seq).
pub trait Clock: Send + Sync {
    /// Milliseconds since the Unix epoch.
    fn now_ms(&self) -> u64;
}

/// The system clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

/// A manually advanced clock — deterministic tests and the golden corpus. `Clone`
/// gives the caller a handle while the store owns a copy (lease-expiry tests).
#[derive(Clone)]
pub struct ManualClock {
    now_ms: std::sync::Arc<Mutex<u64>>,
}

impl ManualClock {
    /// A clock frozen at `ms`.
    pub fn at(ms: u64) -> ManualClock {
        ManualClock {
            now_ms: std::sync::Arc::new(Mutex::new(ms)),
        }
    }

    /// Advance the clock.
    pub fn advance(&self, by_ms: u64) {
        *self.now_ms.lock().unwrap() += by_ms;
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        *self.now_ms.lock().unwrap()
    }
}

/// The allocated-id source. `open_run`, `acquire_writer` and the scope-opening helpers
/// draw ids through it; injectable so the golden corpus is byte-stable.
pub trait IdSource: Send + Sync {
    /// Allocate a fresh id of the given kind (`"run"`, `"evt"`, `"lease"`, `"turn"`,
    /// `"mc"`, `"tc"`).
    fn alloc(&self, kind: &str) -> String;
}

/// Time-ordered ids: `{kind}-{millis:013}-{counter:06}-{tag}` — lexicographically
/// sortable by creation time (fixed-width fields), unique per store via `tag`.
pub struct TimeIds {
    clock: Box<dyn Clock>,
    tag: String,
    counter: AtomicU64,
}

impl TimeIds {
    /// Ids sorted by `clock` then allocation order, tagged `tag` (the store tag).
    pub fn new(clock: Box<dyn Clock>, tag: impl Into<String>) -> TimeIds {
        TimeIds {
            clock,
            tag: tag.into(),
            counter: AtomicU64::new(0),
        }
    }
}

impl IdSource for TimeIds {
    fn alloc(&self, kind: &str) -> String {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        format!("{}-{:013}-{:06}-{}", kind, self.clock.now_ms(), n, self.tag)
    }
}

/// Deterministic sequential ids (`run-000001`, `evt-000002`, …) — the test/golden source.
pub struct SeqIds {
    next: AtomicU64,
}

impl SeqIds {
    /// Start at 1.
    pub fn new() -> SeqIds {
        SeqIds {
            next: AtomicU64::new(1),
        }
    }

    /// Start at `n` — a reopened test store's counter must not re-mint ids
    /// the WAL already holds (production's `TimeIds` is collision-free by
    /// clock+tag; `SeqIds` is deterministic-only).
    pub fn starting_at(n: u64) -> SeqIds {
        SeqIds {
            next: AtomicU64::new(n),
        }
    }
}

impl Default for SeqIds {
    fn default() -> Self {
        SeqIds::new()
    }
}

impl IdSource for SeqIds {
    fn alloc(&self, kind: &str) -> String {
        format!("{}-{:06}", kind, self.next.fetch_add(1, Ordering::Relaxed))
    }
}

/// `effect_id = f(run_id, model_call_id, tool_call_id, ordinal)` — a **derived** id over
/// `idp/1` (ADR-0027 §2): deterministic, so a re-parsed model response can never mint a
/// second identity for the same intent.
pub fn effect_id(run_id: &str, model_call_id: &str, tool_call_id: &str, ordinal: u64) -> String {
    let preimage = Json::Arr(vec![
        Json::str(run_id),
        Json::str(model_call_id),
        Json::str(tool_call_id),
        Json::Int(ordinal as i64),
    ]);
    idp_id("ledger.effect", preimage.to_canonical_string().as_bytes())
}

/// `ts` validation — `RFC 3339, UTC, millisecond precision`: `YYYY-MM-DDTHH:MM:SS.mmmZ`.
/// Strict shape check (the ledger stores the string; it never parses a calendar).
pub fn valid_ts(ts: &str) -> bool {
    let b = ts.as_bytes();
    if b.len() != 24 {
        return false;
    }
    //                 0         1         2
    //                 012345678901234567890123
    //                 2026-09-15T10:20:30.123Z
    let digit = |i: usize| b[i].is_ascii_digit();
    for i in [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18, 20, 21, 22] {
        if !digit(i) {
            return false;
        }
    }
    b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && b[16] == b':'
        && b[19] == b'.'
        && b[23] == b'Z'
        && b[5..=6].iter().all(|c| c.is_ascii_digit())
        && (b[5] != b'1' || b[6] <= b'2') // month ≤ 12
        && (b[11] != b'2' || b[12] <= b'3') // hour ≤ 23
        && b[14] <= b'5' // minute
        && b[17] <= b'5' // second
}

/// Whether `id` is a pinned identity id (`<algo>:<hex>` under `idp/1`) — the
/// `UnresolvedRef`/`ConfigurationUnresolvable` gate's check (a mutable tag fails it).
pub fn is_pinned_id(id: &str) -> bool {
    hh_identity::idp::parse_id(id).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_ids_are_creation_time_sortable() {
        let clock = ManualClock::at(1_000);
        let ids = TimeIds::new(Box::new(ManualClock::at(1_000)), "st");
        let _ = clock; // TimeIds owns its own clock
        let a = ids.alloc("run");
        let b = ids.alloc("run");
        assert!(a < b);
        assert!(a.starts_with("run-0000000001000-"));
    }

    #[test]
    fn seq_ids_are_deterministic() {
        let ids = SeqIds::new();
        assert_eq!(ids.alloc("run"), "run-000001");
        assert_eq!(ids.alloc("evt"), "evt-000002");
    }

    #[test]
    fn effect_id_is_deterministic_and_derived() {
        let a = effect_id("run-1", "mc-1", "tc-1", 0);
        let b = effect_id("run-1", "mc-1", "tc-1", 0);
        assert_eq!(a, b);
        assert!(a.starts_with("sha256:"));
        assert_ne!(a, effect_id("run-1", "mc-1", "tc-1", 1));
    }

    #[test]
    fn ts_shape() {
        assert!(valid_ts("2026-09-15T10:20:30.123Z"));
        assert!(!valid_ts("2026-09-15T10:20:30Z"));
        assert!(!valid_ts("2026-09-15 10:20:30.123Z"));
        assert!(!valid_ts("2026-13-15T10:20:30.123Z"));
        assert!(!valid_ts("2026-09-15T24:20:30.123Z"));
    }
}
