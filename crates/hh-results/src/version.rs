//! `RowVersion` — `{key, version_id, scoring, supersedes?, derived_from_reason}`
//! (§6.5 §3; ADR-0161 D2). Versions are immutable superseding projections
//! under the fixed `key`; `head(key)` is the newest recorded version; no
//! version is ever deleted or mutated (`rescore` refuses `NoChange`).

use std::collections::BTreeMap;

use hh_wire::json::Json;

use crate::error::ResultsError;
use crate::scoring::ScoringContext;
use crate::watermark::WatermarkSet;

/// `derived_from_reason ∈ {initial, regrade, validator_superseded,
/// pricing_resnapshot, registry_bump, redaction}` (§6.5 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerivedReason {
    /// The first projection of the run.
    Initial,
    /// An overlay regrade.
    Regrade,
    /// A validator in the scoring set was superseded.
    ValidatorSuperseded,
    /// A pricing snapshot moved.
    PricingResnapshot,
    /// The pinned metric registry moved.
    RegistryBump,
    /// Evidence redaction forced a re-derivation.
    Redaction,
}

impl DerivedReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DerivedReason::Initial => "initial",
            DerivedReason::Regrade => "regrade",
            DerivedReason::ValidatorSuperseded => "validator_superseded",
            DerivedReason::PricingResnapshot => "pricing_resnapshot",
            DerivedReason::RegistryBump => "registry_bump",
            DerivedReason::Redaction => "redaction",
        }
    }

    /// Parse a canonical spelling.
    pub fn parse(s: &str) -> Option<DerivedReason> {
        Some(match s {
            "initial" => DerivedReason::Initial,
            "regrade" => DerivedReason::Regrade,
            "validator_superseded" => DerivedReason::ValidatorSuperseded,
            "pricing_resnapshot" => DerivedReason::PricingResnapshot,
            "registry_bump" => DerivedReason::RegistryBump,
            "redaction" => DerivedReason::Redaction,
            _ => return None,
        })
    }
}

/// `RowVersion{key, version_id, scoring, supersedes?, derived_from_reason,
/// watermark_set}` — the version record under a fixed row key.
#[derive(Debug, Clone, PartialEq)]
pub struct RowVersion {
    /// The fixed row key (`{configuration_version_id, run_id}` canonical
    /// JSON — the same record the row carries).
    pub key: Json,
    /// The row version's identity (`idp/1` over the row's canonical bytes).
    pub version_id: String,
    /// The scoring context the version was projected under.
    pub scoring: ScoringContext,
    /// The superseded head version (`None` on the root).
    pub supersedes: Option<String>,
    /// Why the version was derived.
    pub derived_from_reason: DerivedReason,
    /// The watermark set the version was projected at.
    pub watermark_set: WatermarkSet,
}

impl RowVersion {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("key".into(), self.key.clone());
        m.insert("version_id".into(), Json::str(&self.version_id));
        m.insert("scoring".into(), self.scoring.to_json());
        if let Some(s) = &self.supersedes {
            m.insert("supersedes".into(), Json::str(s));
        }
        m.insert(
            "derived_from_reason".into(),
            Json::str(self.derived_from_reason.as_str()),
        );
        m.insert("watermark_set".into(), self.watermark_set.to_json());
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<RowVersion, ResultsError> {
        let Json::Obj(m) = j else {
            return Err(ResultsError::Schema {
                member: "row_version".into(),
                detail: "not an object".into(),
            });
        };
        let bad = |member: &str| ResultsError::Schema {
            member: member.to_string(),
            detail: "absent or ill-typed".into(),
        };
        Ok(RowVersion {
            key: m.get("key").cloned().ok_or_else(|| bad("key"))?,
            version_id: m
                .get("version_id")
                .and_then(Json::as_str)
                .ok_or_else(|| bad("version_id"))?
                .to_string(),
            scoring: ScoringContext::from_json(m.get("scoring").ok_or_else(|| bad("scoring"))?)?,
            supersedes: m.get("supersedes").and_then(Json::as_str).map(String::from),
            derived_from_reason: m
                .get("derived_from_reason")
                .and_then(Json::as_str)
                .and_then(DerivedReason::parse)
                .ok_or_else(|| bad("derived_from_reason"))?,
            watermark_set: WatermarkSet::from_json(
                m.get("watermark_set").ok_or_else(|| bad("watermark_set"))?,
            )
            .ok_or_else(|| bad("watermark_set"))?,
        })
    }
}
