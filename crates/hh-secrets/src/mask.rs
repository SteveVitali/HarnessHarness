//! The known-value **mask set** (§5g.3 §2; ADR-0057 D5): the broker's
//! kernel-side table of *every* live channel's value **plus every rotated
//! revision's retained value** (SV-1 — the mask covers all revisions, so a stale
//! copy of a rotated credential still masks). The set is in-memory, kernel-only,
//! and is never serialized — its fingerprints and placeholders are what the
//! audit rows carry.

use std::collections::BTreeMap;

use crate::types::{Placeholder, SecretFingerprint};

/// One mask-set entry: a value's placeholder replacement + fingerprint. The
/// `value` field is **kernel-side only** — it exists so `mask`/`leak_scan` can
/// match, and it is never written to a durable or model-visible surface.
#[derive(Debug, Clone)]
pub struct MaskEntry {
    /// The channel the value belongs to.
    pub channel_id: String,
    /// The channel revision this value was minted under.
    pub revision: u64,
    /// The value itself (kernel-side only — never logged, never serialized).
    pub value: String,
    /// The keyed fingerprint (`security.secret.*` rows carry this, not bytes).
    pub fingerprint: SecretFingerprint,
    /// Whether this is the channel's current revision (`false` = retained
    /// rotated value — still masked; SV-1).
    pub live: bool,
    /// The placeholder a masked occurrence is replaced with, when the masking
    /// context knows the binding (masking that isn't binding-scoped uses the
    /// channel-level tombstone form `${SECRET:<channel_id>}` — the Stage-0
    /// placeholder spelling the baseline established).
    pub placeholder: Option<Placeholder>,
}

/// `MaskSet` — the broker's coverage of every live channel plus retained
/// rotated revisions. Built by [`crate::broker::CredentialBroker::mask_set`]
/// (which resolves each channel's source once — a source failure fails the
/// build closed, never silently drops the channel: SV-1's "every live channel"
/// is a completeness obligation).
#[derive(Debug, Clone, Default)]
pub struct MaskSet {
    /// `channel_id → entries` (all revisions; `live` marks the current one).
    pub entries: BTreeMap<String, Vec<MaskEntry>>,
}

impl MaskSet {
    /// Every `(channel_id, value)` pair, longest-value first (so a value that is
    /// a prefix of another masks completely — the scan is total, not
    /// first-match).
    pub fn values_desc(&self) -> Vec<(&str, &str)> {
        let mut v: Vec<(&str, &str)> = self
            .entries
            .iter()
            .flat_map(|(cid, es)| es.iter().map(move |e| (cid.as_str(), e.value.as_str())))
            .collect();
        v.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
        v
    }

    /// All entries across channels (order: channel, then live-first).
    pub fn all_entries(&self) -> Vec<&MaskEntry> {
        self.entries.values().flatten().collect()
    }

    /// The entry a value belongs to, if any.
    pub fn entry_for(&self, value: &str) -> Option<&MaskEntry> {
        self.all_entries().into_iter().find(|e| e.value == value)
    }

    /// Insert an entry (broker-internal).
    pub fn insert(&mut self, e: MaskEntry) {
        self.entries
            .entry(e.channel_id.clone())
            .or_default()
            .push(e);
    }

    /// Whether the set has an entry for `channel_id` at any revision.
    pub fn covers(&self, channel_id: &str) -> bool {
        self.entries
            .get(channel_id)
            .is_some_and(|es| !es.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_set_orders_longest_first_and_covers_channels() {
        let mut ms = MaskSet::default();
        for (v, live) in [("abc123def456", true), ("abc123def456extended", false)] {
            ms.insert(MaskEntry {
                channel_id: "ch".into(),
                revision: if live { 2 } else { 1 },
                value: v.into(),
                fingerprint: SecretFingerprint {
                    channel_id: "ch".into(),
                    revision: if live { 2 } else { 1 },
                    digest: "0".repeat(32),
                },
                live,
                placeholder: None,
            });
        }
        assert!(ms.covers("ch"));
        assert!(!ms.covers("other"));
        let vals = ms.values_desc();
        assert_eq!(vals[0].1, "abc123def456extended"); // longest first
        assert_eq!(ms.entry_for("abc123def456").unwrap().revision, 2);
    }
}
