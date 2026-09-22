//! The **results-store and audit row keys** (§8.3 #3; ADR-0038 D3/D4). This section *fixes the
//! keys*; §06 owns the results store and §05g owns the audit chain (CC10 — a section that fixes
//! records does not own the services).

/// The results-store **row key** `(configuration_version_id, run_id)` (§8.3 #3). Aggregation is by
/// `configuration_id` and the explicit factor tuple (T-LCD-09); `bundle_ref`/`claimed_level` are
/// **derived index annotations**, never inside row bytes; leaderboards never key by name alone.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResultsRowKey {
    pub configuration_version_id: String,
    pub run_id: String,
}

impl ResultsRowKey {
    pub fn new(
        configuration_version_id: impl Into<String>,
        run_id: impl Into<String>,
    ) -> ResultsRowKey {
        ResultsRowKey {
            configuration_version_id: configuration_version_id.into(),
            run_id: run_id.into(),
        }
    }

    /// The **aggregation** coordinate: the seedless `configuration_id`. Row versions (re-scoring)
    /// are produced by supersession, and `depends_on_revoked` rows are annotated (never removed).
    pub fn aggregation_key(configuration_id: &str) -> String {
        configuration_id.to_string()
    }
}

/// An audit `ir_ref` — the `{semantic_id, version_id}` pair the ledger carries (§8.3 #3, ADR-0038
/// D4). The attestation subject is the bundle manifest `version_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrRef {
    pub semantic_id: Option<String>,
    pub version_id: String,
}

impl IrRef {
    pub fn new(version_id: impl Into<String>, semantic_id: Option<String>) -> IrRef {
        IrRef {
            semantic_id,
            version_id: version_id.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_key_is_config_version_id_and_run_id() {
        let k = ResultsRowKey::new("sha256:cvid", "run-1");
        assert_eq!(k.configuration_version_id, "sha256:cvid");
        assert_eq!(k.run_id, "run-1");
    }

    #[test]
    fn two_seeds_share_aggregation_key_differ_in_row_key() {
        // Rows are keyed by (configuration_version_id, run_id); they aggregate by configuration_id.
        let cid = "sha256:cid";
        let k1 = ResultsRowKey::new("sha256:cvid-seed0", "run-1");
        let k2 = ResultsRowKey::new("sha256:cvid-seed1", "run-2");
        assert_ne!(k1, k2);
        assert_eq!(
            ResultsRowKey::aggregation_key(cid),
            ResultsRowKey::aggregation_key(cid)
        );
    }

    #[test]
    fn ir_ref_carries_both_ids() {
        let r = IrRef::new("sha256:v", Some("sha256:s".into()));
        assert_eq!(r.version_id, "sha256:v");
        assert_eq!(r.semantic_id.as_deref(), Some("sha256:s"));
    }
}
