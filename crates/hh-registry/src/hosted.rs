//! The hosted capability-vector reconcile (§6.6 §9; R-2.10.6; S4.5a).
//!
//! `capability_vector` is a **reconciled read**, never authored: per dimension,
//! the latest conformance record that is not stale wins; absent a record the
//! declaration's own value stands; absent both, `unknown`. Stale records
//! (pinned to an earlier `participant_version_identity`) are reported, never
//! hidden — a version change reverts the vector to the declaration until
//! re-probed (AC-R-2.10.6-10). `drift` is data: a non-P0 drift annotates the
//! stratum (P0 quarantine is the boundary's act — `RegistryStore::quarantine`).
//!
//! This module is **records-in/records-out** — it reads `ConformanceRecord`s
//! the caller collected ([`RegistryStore::hosted_conformance_entries`]) and the
//! declaration map off the participant record body. No `hh-hosting` dependency:
//! the reconcile speaks the registry's own types (CC5/CC6 — `hh-hosting`
//! stays a removable tier).

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_wire::json::Json;

use crate::kinds::ConformanceVerdict;
use crate::records::ConformanceRecord;

/// A reconciled capability vector (§6.6 §9.2): `vector` carries the effective
/// per-dimension value; `verdicts` the reconciled verdict per dimension;
/// `drift_dimensions` every non-stale DRIFT (the quarantine set is the
/// boundary's P0 filtering, not this module's); `stale_dimensions` every
/// dimension whose records all pin a superseded `version_identity`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CapabilityVector {
    /// `dimension → effective value` (an entry's `observed`, or the
    /// declaration's value, or `"unknown"`).
    pub vector: BTreeMap<String, Json>,
    /// `dimension → reconciled verdict`.
    pub verdicts: BTreeMap<String, ConformanceVerdict>,
    /// Non-stale entries whose verdict is `drift` (sorted, deduplicated).
    pub drift_dimensions: Vec<String>,
    /// Dimensions whose *only* entries are stale (sorted, deduplicated).
    pub stale_dimensions: Vec<String>,
}

/// `reconcile_capability_vector(declaration, entries, current_identity)` — the
/// §6.6 §9.2 rule. `entries` are read in observation order (`at` seqs); the
/// last non-stale entry per dimension wins.
pub fn reconcile_capability_vector(
    declaration: &BTreeMap<String, Json>,
    entries: &[ConformanceRecord],
    current_version_identity: &str,
) -> CapabilityVector {
    let mut fresh: BTreeMap<&str, &ConformanceRecord> = BTreeMap::new();
    let mut stale_dims: BTreeSet<String> = BTreeSet::new();
    let mut drift: BTreeSet<String> = BTreeSet::new();
    for e in entries {
        if e.is_stale(current_version_identity) {
            stale_dims.insert(e.dimension.clone());
            continue;
        }
        stale_dims.remove(&e.dimension);
        fresh.insert(e.dimension.as_str(), e);
        if e.verdict == ConformanceVerdict::Drift {
            drift.insert(e.dimension.clone());
        } else {
            // A later non-drift observation supersedes an earlier drift's
            // annotation for that dimension — the *latest* record speaks.
            drift.remove(&e.dimension);
        }
    }
    // Dimensions whose only entries are stale still report as stale.
    for e in entries {
        if e.is_stale(current_version_identity) && !fresh.contains_key(e.dimension.as_str()) {
            stale_dims.insert(e.dimension.clone());
        }
    }
    let mut vector = BTreeMap::new();
    let mut verdicts = BTreeMap::new();
    let dims: BTreeSet<&str> = declaration
        .keys()
        .map(String::as_str)
        .chain(fresh.keys().copied())
        .collect();
    for dim in dims {
        if let Some(e) = fresh.get(dim) {
            vector.insert(dim.to_string(), e.observed.clone());
            verdicts.insert(dim.to_string(), e.verdict);
        } else if let Some(v) = declaration.get(dim) {
            vector.insert(dim.to_string(), v.clone());
            verdicts.insert(
                dim.to_string(),
                v.as_str()
                    .and_then(ConformanceRecord::verdict_parse)
                    .unwrap_or(ConformanceVerdict::Unknown),
            );
        } else {
            vector.insert(dim.to_string(), Json::str("unknown"));
            verdicts.insert(dim.to_string(), ConformanceVerdict::Unknown);
        }
    }
    CapabilityVector {
        vector,
        verdicts,
        drift_dimensions: drift.into_iter().collect(),
        stale_dimensions: stale_dims.into_iter().collect(),
    }
}

/// The one capability-gate read (§6.6 §9.2): `vector[dimension]` is concretely
/// `supported` — `partial` is **not** support for an adapter gate (a partial
/// verb may still be attempted only where the spec says so — the gate is
/// strict).
pub fn vector_supports(vector: &CapabilityVector, dimension: &str) -> bool {
    matches!(
        vector.verdicts.get(dimension),
        Some(ConformanceVerdict::Supported)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::ObservedIn;

    fn entry(
        dimension: &str,
        declared: &str,
        observed: &str,
        identity: &str,
        at: u64,
    ) -> ConformanceRecord {
        ConformanceRecord::observed_entry(
            identity,
            "adapter:a1",
            dimension,
            Json::str(declared),
            Json::str(observed),
            ObservedIn::Probe,
            None,
            at,
        )
    }

    #[test]
    fn the_reconcile_is_latest_non_stale_then_declaration_then_unknown() {
        let declaration: BTreeMap<String, Json> = [
            ("interrupt".into(), Json::str("supported")),
            ("steer".into(), Json::str("unsupported")),
        ]
        .into_iter()
        .collect();
        let entries = vec![
            entry("interrupt", "supported", "unsupported", "p:v1", 10),
            entry("interrupt", "supported", "supported", "p:v1", 20), // latest wins
            entry("streaming", "supported", "supported", "p:v0", 30), // stale
        ];
        let v = reconcile_capability_vector(&declaration, &entries, "p:v1");
        assert_eq!(
            v.verdicts["interrupt"],
            ConformanceVerdict::Supported,
            "the latest non-stale record speaks"
        );
        assert_eq!(v.vector["interrupt"], Json::str("supported"));
        assert_eq!(
            v.verdicts["steer"],
            ConformanceVerdict::Unsupported,
            "the declaration's own value stands unprobed"
        );
        assert!(
            !v.verdicts.contains_key("streaming"),
            "a dimension carried only by stale records is not part of the \
             current vector — the staleness is the report, not a member"
        );
        assert_eq!(v.stale_dimensions, vec!["streaming".to_string()]);
        assert!(v.drift_dimensions.is_empty());
    }

    #[test]
    fn drift_is_data_and_a_later_clean_observation_supersedes_it() {
        let entries = vec![
            entry("policy_ask", "supported", "unsupported", "p:v1", 10),
            entry("basic_turn", "supported", "supported", "p:v1", 11),
            entry("policy_ask", "supported", "supported", "p:v1", 12),
        ];
        let v = reconcile_capability_vector(&BTreeMap::new(), &entries, "p:v1");
        assert!(v.drift_dimensions.is_empty(), "latest record wins");
        let only_drift = reconcile_capability_vector(&BTreeMap::new(), &entries[..1], "p:v1");
        assert_eq!(only_drift.drift_dimensions, vec!["policy_ask".to_string()]);
    }

    #[test]
    fn skipped_and_unknown_never_drift() {
        let entries = vec![
            entry("context_delivery", "supported", "skipped", "p:v1", 1),
            entry("usage_reporting", "supported", "unknown", "p:v1", 2),
        ];
        let v = reconcile_capability_vector(&BTreeMap::new(), &entries, "p:v1");
        assert_eq!(v.verdicts["context_delivery"], ConformanceVerdict::Skipped);
        assert_eq!(v.verdicts["usage_reporting"], ConformanceVerdict::Unknown);
        assert!(v.drift_dimensions.is_empty());
    }
}
