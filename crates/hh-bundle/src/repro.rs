//! `ReproReport` — the `reproduce(bundle_ref, level, {seed, eval_budget})`
//! output record (§5h.3 §2/§4). The driver lives in `hh-embed` (it owns
//! replay machinery); this module owns the record shape and the pure
//! compares the driver calls: `budget_limits_equal` (the `UnmatchedBudget`
//! refusal, T-LCD-14) and `model_fingerprint`/`fingerprint_drift` (the
//! R3 weight drift probe).

use std::collections::BTreeMap;

use hh_wire::json::Json;

use crate::manifest::{BundleManifest, ReproLevel};

/// The report's schema id.
pub const REPRO_SCHEMA: &str = "hh-repro/1";

/// `reproduce` outcome — `pass | drift | nondeterministic | inconclusive
/// | refused`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReproOutcome {
    /// The level's checks held.
    Pass,
    /// A byte/oracle/fingerprint comparison differed.
    Drift,
    /// A re-run diverged inside a declared nondeterminism class.
    Nondeterministic,
    /// Materialized evidence cannot decide (a member is unpinned and the
    /// level still ran — e.g. a `fetch` member the driver could not get).
    Inconclusive,
    /// The call refused (`LevelUnsupported`, `UnmatchedBudget`,
    /// `EnvironmentUnavailable`, `ModelUnavailable`).
    Refused,
}

impl ReproOutcome {
    /// Canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            ReproOutcome::Pass => "pass",
            ReproOutcome::Drift => "drift",
            ReproOutcome::Nondeterministic => "nondeterministic",
            ReproOutcome::Inconclusive => "inconclusive",
            ReproOutcome::Refused => "refused",
        }
    }
}

/// `ReproReport{bundle_id, requested_level, achieved_level?, outcome,
/// refusal?, evidence{oracle_diff[], fingerprints[], budget_match},
/// drift[], basis[], environment_ok, model_ok, report_id}`.
#[derive(Debug, Clone, PartialEq)]
pub struct ReproReport {
    /// The subject bundle.
    pub bundle_id: String,
    /// The requested level.
    pub requested_level: String,
    /// The achieved level (`None` on refusal).
    pub achieved_level: Option<String>,
    /// The verdict.
    pub outcome: ReproOutcome,
    /// The refusal reason (outcome `refused`).
    pub refusal: Option<String>,
    /// `{oracle_diff[], fingerprints[], budget_match}`.
    pub evidence: Json,
    /// The `DriftReport[]` rows.
    pub drift: Vec<Json>,
    /// The `LevelBasis[]` the report ran against.
    pub basis: Vec<Json>,
    /// The environment could be provisioned.
    pub environment_ok: bool,
    /// The pinned/fingerprinted model is the one that ran.
    pub model_ok: bool,
    /// `idp/1` over the report minus `report_id`.
    pub report_id: String,
}

impl ReproReport {
    /// Canonical JSON (report_id stamped).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("schema".into(), Json::str(REPRO_SCHEMA));
        m.insert("bundle_id".into(), Json::str(self.bundle_id.clone()));
        m.insert(
            "requested_level".into(),
            Json::str(self.requested_level.clone()),
        );
        m.insert(
            "achieved_level".into(),
            self.achieved_level
                .clone()
                .map(Json::str)
                .unwrap_or(Json::Null),
        );
        m.insert("outcome".into(), Json::str(self.outcome.name()));
        if let Some(r) = &self.refusal {
            m.insert("refusal".into(), Json::str(r.clone()));
        }
        m.insert("evidence".into(), self.evidence.clone());
        m.insert("drift".into(), Json::Arr(self.drift.clone()));
        m.insert("basis".into(), Json::Arr(self.basis.clone()));
        m.insert("environment_ok".into(), Json::Bool(self.environment_ok));
        m.insert("model_ok".into(), Json::Bool(self.model_ok));
        m.insert("report_id".into(), Json::str(self.report_id.clone()));
        Json::Obj(m)
    }
    /// Stamp `report_id`.
    pub fn seal(&mut self) {
        self.report_id = String::new();
        let mut doc = match self.to_json() {
            Json::Obj(m) => m,
            _ => unreachable!(),
        };
        doc.remove("report_id");
        self.report_id = hh_identity::idp_id(
            "bundle.repro",
            Json::Obj(doc).to_canonical_string().as_bytes(),
        );
    }
    /// A refusal report (the `refused` outcome).
    pub fn refused(bundle_id: &str, level: ReproLevel, reason: impl Into<String>) -> ReproReport {
        let mut r = ReproReport {
            bundle_id: bundle_id.to_string(),
            requested_level: level.name().to_string(),
            achieved_level: None,
            outcome: ReproOutcome::Refused,
            refusal: Some(reason.into()),
            evidence: Json::obj([
                ("oracle_diff", Json::Arr(vec![])),
                ("fingerprints", Json::Arr(vec![])),
                ("budget_match", Json::Null),
            ]),
            drift: vec![],
            basis: vec![],
            environment_ok: false,
            model_ok: false,
            report_id: String::new(),
        };
        r.seal();
        r
    }
}

/// `eval_budget` equality — the T-LCD-14 `UnmatchedBudget` gate. Two
/// `eval_budget` documents match iff their canonical `limits` (the whole
/// document when it has no `limits` member) are canonically equal.
pub fn budget_limits_equal(declared: &Json, requested: &Json) -> bool {
    let norm = |j: &Json| -> Json { j.get("limits").cloned().unwrap_or_else(|| j.clone()) };
    norm(declared).to_canonical_string() == norm(requested).to_canonical_string()
}

/// A model snapshot's fingerprint — `idp/1` over the snapshot's pinned
/// members (`{model_id, snapshot_id, weights}` — whatever the snapshot
/// declares; `None` when it declares nothing hashable). The fingerprint
/// is what an unpinned `pinned: false` snapshot can still carry for the
/// R3 drift probe (AC: "R3 only with fingerprints").
pub fn snapshot_fingerprint(snapshot: &Json) -> Option<String> {
    snapshot
        .get("observed_fingerprint")
        .and_then(Json::as_str)
        .map(String::from)
        .or_else(|| {
            let material = Json::obj([
                (
                    "model_id",
                    snapshot.get("model_id").cloned().unwrap_or(Json::Null),
                ),
                (
                    "snapshot_id",
                    snapshot.get("snapshot_id").cloned().unwrap_or(Json::Null),
                ),
                (
                    "weights",
                    snapshot.get("weights").cloned().unwrap_or(Json::Null),
                ),
            ]);
            if material.to_canonical_string().contains("sha256:") {
                Some(hh_identity::idp_id(
                    "model.fingerprint",
                    material.to_canonical_string().as_bytes(),
                ))
            } else {
                None
            }
        })
}

/// The drift rows between two fingerprint sets — `{model_ref, declared,
/// observed}` per mismatch.
pub fn fingerprint_drift(declared: &BundleManifest, observed: &[(String, String)]) -> Vec<Json> {
    let mut drift = Vec::new();
    if let Some(Json::Arr(snaps)) = declared.model.get("snapshots") {
        for (i, s) in snaps.iter().enumerate() {
            let declared_fp = s
                .get("observed_fingerprint")
                .and_then(Json::as_str)
                .unwrap_or("");
            let model_ref = s
                .get("model_id")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string();
            if let Some((_, obs)) = observed.iter().find(|(r, _)| r == &model_ref) {
                if obs != declared_fp {
                    drift.push(Json::obj([
                        ("kind", Json::str("model_fingerprint")),
                        ("model_ref", Json::str(model_ref.clone())),
                        ("declared", Json::str(declared_fp)),
                        ("observed", Json::str(obs.clone())),
                        ("snapshot_index", Json::Int(i as i64)),
                    ]));
                }
            }
        }
    }
    drift
}
