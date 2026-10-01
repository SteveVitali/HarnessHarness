//! `ScoringContext` — the pinned scoring inputs a row is projected under
//! (§6.5 data model: `scoring: ScoringContext{validator_set[], overlay_runs[],
//! metric_registry_version, pricing_ref, view_policy_version}`; ADR-0161 D1).
//! The context is data on the row — two projections under different contexts
//! are different versions, never a mutation.

use hh_wire::json::Json;

use crate::error::ResultsError;

/// The view-policy version this build implements (the deterministic
/// projection ruleset — bumping it is a `registry_bump`-class re-score).
pub const VIEW_POLICY_VERSION: &str = "hh-results-view/1";

/// `overlay_runs[]` member — `{run_id, until_seq}` — an overlay (regrade)
/// run read up to its own watermark (ADR-0161 D2: the overlay is an ordinary
/// instrument run; it never appends to the subject's ledger).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayRef {
    /// The overlay run.
    pub run_id: String,
    /// The overlay's read watermark (`None` = its head at projection time —
    /// the watermark the row records is the resolved seq).
    pub until_seq: Option<u64>,
}

impl OverlayRef {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert("run_id".into(), Json::str(&self.run_id));
        if let Some(s) = self.until_seq {
            m.insert("until_seq".into(), Json::Int(s as i64));
        }
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Option<OverlayRef> {
        let Json::Obj(m) = j else {
            return None;
        };
        let run_id = m.get("run_id")?.as_str()?.to_string();
        let until_seq = match m.get("until_seq") {
            Some(v) => Some(v.as_int()? as u64),
            None => None,
        };
        Some(OverlayRef { run_id, until_seq })
    }
}

/// `ScoringContext` — the row's pinned scoring environment.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoringContext {
    /// The admitted validator refs (empty = the deterministic default set —
    /// overlay verdicts still bind by `audit_ref`, not by validator).
    pub validator_set: Vec<String>,
    /// The overlay (regrade) runs folded into this projection.
    pub overlay_runs: Vec<OverlayRef>,
    /// The pinned metric registry version (`hh_eval::catalogue`'s
    /// `metric_registry_version` at Stage 3 — the only registry this build
    /// carries; anything else is `RegistryVersionUnknown`).
    pub metric_registry_version: String,
    /// The pricing snapshot the spend cells name (`None` at Stage 3 when the
    /// run carried no priced consumption).
    pub pricing_ref: Option<String>,
    /// The projection ruleset version.
    pub view_policy_version: String,
}

impl ScoringContext {
    /// The default context — `native{registry_version}` per §6.5 §2.1: the
    /// compiled-in metric catalogue's content address and this build's view
    /// policy.
    pub fn native() -> ScoringContext {
        ScoringContext {
            validator_set: Vec::new(),
            overlay_runs: Vec::new(),
            metric_registry_version: hh_eval::catalogue::check_catalogue().metric_registry_version,
            pricing_ref: None,
            view_policy_version: VIEW_POLICY_VERSION.to_string(),
        }
    }

    /// Validate the pin — `RegistryVersionUnknown` when the named registry
    /// is not this build's catalogue.
    pub fn validate(&self) -> Result<(), ResultsError> {
        let known = hh_eval::catalogue::check_catalogue().metric_registry_version;
        if self.metric_registry_version != known {
            return Err(ResultsError::RegistryVersionUnknown {
                found: self.metric_registry_version.clone(),
                known,
            });
        }
        Ok(())
    }

    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = std::collections::BTreeMap::new();
        m.insert(
            "validator_set".into(),
            Json::Arr(self.validator_set.iter().map(Json::str).collect()),
        );
        m.insert(
            "overlay_runs".into(),
            Json::Arr(self.overlay_runs.iter().map(|o| o.to_json()).collect()),
        );
        m.insert(
            "metric_registry_version".into(),
            Json::str(&self.metric_registry_version),
        );
        m.insert(
            "pricing_ref".into(),
            self.pricing_ref.as_ref().map_or(Json::Null, Json::str),
        );
        m.insert(
            "view_policy_version".into(),
            Json::str(&self.view_policy_version),
        );
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<ScoringContext, ResultsError> {
        let Json::Obj(m) = j else {
            return Err(ResultsError::Schema {
                member: "scoring".into(),
                detail: "not an object".into(),
            });
        };
        let bad = |member: &str| ResultsError::Schema {
            member: member.to_string(),
            detail: "ill-typed".into(),
        };
        let validator_set = match m.get("validator_set") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(|i| i.as_str().map(String::from))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| bad("validator_set"))?,
            _ => Vec::new(),
        };
        let overlay_runs = match m.get("overlay_runs") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(OverlayRef::from_json)
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| bad("overlay_runs"))?,
            _ => Vec::new(),
        };
        let metric_registry_version = m
            .get("metric_registry_version")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("metric_registry_version"))?
            .to_string();
        let pricing_ref = m
            .get("pricing_ref")
            .and_then(Json::as_str)
            .map(String::from);
        let view_policy_version = m
            .get("view_policy_version")
            .and_then(Json::as_str)
            .unwrap_or(VIEW_POLICY_VERSION)
            .to_string();
        Ok(ScoringContext {
            validator_set,
            overlay_runs,
            metric_registry_version,
            pricing_ref,
            view_policy_version,
        })
    }
}
