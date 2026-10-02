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
/// refusal?, verdict, reproducer, reproducer_instrument, independent,
/// evidence{oracle_diff[], fingerprints[], budget_match},
/// drift[], basis[], environment_ok, model_ok, report_id}` (§5h.3 §2's
/// `ReproReport{bundle_id, level, independent, verdict,
/// per_member_hash_checks[], per_validator_agreement[],
/// end_state_diff_ref?, distributions?, nondeterminism_sources_observed[],
/// budget_match, seeds[], cost, environment_ok, model_ok, reproducer:
/// ProvenanceRecord, reproducer_instrument{version_id,
/// component_versions, installation_id}}` — S4.4, R-2.9.3 C1/Stage-4
/// "`ReproReport` sidecar with `independent`").
///
/// `verdict` is the spec's four-valued spelling
/// (`reproduced | not_reproduced | inconclusive | n/a{reason}`) derived
/// from `outcome` at serialisation — `outcome` stays as the interim C0
/// spelling (CC8 additive; the `set_status{to: reproduced}` gate and
/// imported codecs read `verdict`). `independent` is **kernel-computed
/// by [`compute_independence`]** — never read from the request and
/// never minted by a codec: repeatability ≠ reproducibility (OQ-334
/// interim rule, ADR-0295).
#[derive(Debug, Clone, PartialEq)]
pub struct ReproReport {
    /// The subject bundle.
    pub bundle_id: String,
    /// The requested level.
    pub requested_level: String,
    /// The achieved level (`None` on refusal).
    pub achieved_level: Option<String>,
    /// The outcome (C0 spelling — `verdict` is the spec vocabulary).
    pub outcome: ReproOutcome,
    /// The refusal reason (outcome `refused`).
    pub refusal: Option<String>,
    /// The reproducer's declared `ProvenanceRecord` JSON (`Json::Null`
    /// when the call declared none — the kernel's own provenance is
    /// stamped by the driver in that case).
    pub reproducer: Json,
    /// The reproducer's declared
    /// `InstrumentRecord{version_id, component_versions?, installation_id?}`
    /// JSON (`Json::Null` when undeclared).
    pub reproducer_instrument: Json,
    /// `true` only when the reproducer's declared signer AND
    /// installation both resolve and BOTH differ from the producer's
    /// declared material ([`compute_independence`]; fail-closed —
    /// undeclared or equal material computes `false`).
    pub independent: bool,
    /// `{oracle_diff[], fingerprints[], budget_match,
    /// independent_basis}`.
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
        m.insert("verdict".into(), self.verdict_json());
        m.insert("reproducer".into(), self.reproducer.clone());
        m.insert(
            "reproducer_instrument".into(),
            self.reproducer_instrument.clone(),
        );
        m.insert("independent".into(), Json::Bool(self.independent));
        m.insert("evidence".into(), self.evidence.clone());
        m.insert("drift".into(), Json::Arr(self.drift.clone()));
        m.insert("basis".into(), Json::Arr(self.basis.clone()));
        m.insert("environment_ok".into(), Json::Bool(self.environment_ok));
        m.insert("model_ok".into(), Json::Bool(self.model_ok));
        m.insert("report_id".into(), Json::str(self.report_id.clone()));
        Json::Obj(m)
    }
    /// The spec's four-valued `verdict` member — derived from `outcome`,
    /// never stored separately (CC1): `pass → reproduced`,
    /// `drift → not_reproduced`, `nondeterministic | inconclusive →
    /// inconclusive`, `refused → n/a{reason}` (the `MetricValueKind::Na`
    /// spelling `{"kind": "na", "reason": …}` — refusal is not a
    /// reproduction verdict).
    pub fn verdict_json(&self) -> Json {
        match self.outcome {
            ReproOutcome::Pass => Json::str("reproduced"),
            ReproOutcome::Drift => Json::str("not_reproduced"),
            ReproOutcome::Nondeterministic | ReproOutcome::Inconclusive => {
                Json::str("inconclusive")
            }
            ReproOutcome::Refused => Json::obj([
                ("kind", Json::str("na")),
                (
                    "reason",
                    Json::str(
                        self.refusal
                            .clone()
                            .unwrap_or_else(|| "refused".to_string()),
                    ),
                ),
            ]),
        }
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
            reproducer: Json::Null,
            reproducer_instrument: Json::Null,
            independent: false,
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

/// The declared signer of a `ProvenanceRecord` JSON — the
/// `attestation.anchor.signer` member (one scheme: a signing claim IS an
/// attestation anchor — CC1). `signer` is never invented from `origin`.
fn declared_signer(provenance: &Json) -> Option<&str> {
    provenance
        .get("attestation")
        .and_then(|a| a.get("anchor"))
        .and_then(|a| a.get("signer"))
        .and_then(Json::as_str)
}

/// `independent` — the §5h.3 `ReproReport` flag (S4.4; OQ-334 interim
/// rule, ADR-0295). A reproduction is `independent` only when the
/// reproducer's declared identity material BOTH resolves AND differs
/// from the producer's on **both** axes:
///
/// - **signer** — `producer.attestation.anchor.signer` vs
///   `reproducer.attestation.anchor.signer`;
/// - **installation** — `manifest.instrument.installation_id` vs
///   `reproducer_instrument.installation_id`.
///
/// The rule is fail-closed: undeclared material on either side computes
/// `false` with the basis named (`*_undeclared`), as does equal material
/// (`same_signer` / `same_installation`). Cryptographic establishment of
/// the reproducer's claim (the §05g "distinct installation and signer,
/// attested" verification — a signed statement from the reproducer's own
/// root) is the C2/Stage-5 item the `reproduced`-status admission needs
/// (R-2.9.3 stage row; S5.4 owns the runtime arm); until then this flag
/// is a declared-material comparison and the gate stays on it.
///
/// Returns `(independent, basis)` — `basis` is recorded on
/// `evidence.independent_basis` so the flag is auditable. `producer` is
/// the manifest's `producer` `ProvenanceRecord` JSON, `instrument` the
/// manifest's `instrument` JSON, `reproducer` /
/// `reproducer_instrument` the declared counterparts.
pub fn compute_independence(
    producer: &Json,
    instrument: &Json,
    reproducer: &Json,
    reproducer_instrument: &Json,
) -> (bool, &'static str) {
    let rep_signer = declared_signer(reproducer);
    let prod_signer = declared_signer(producer);
    let rep_inst = reproducer_instrument
        .get("installation_id")
        .and_then(Json::as_str);
    let prod_inst = instrument.get("installation_id").and_then(Json::as_str);
    match (rep_signer, prod_signer, rep_inst, prod_inst) {
        (Some(rs), Some(ps), Some(ri), Some(pi)) if rs != ps && ri != pi => {
            (true, "distinct_signer_and_installation")
        }
        (Some(rs), Some(ps), _, _) if rs == ps => (false, "same_signer"),
        (None, _, _, _) => (false, "reproducer_signer_undeclared"),
        (_, None, _, _) => (false, "producer_signer_undeclared"),
        (_, _, None, _) => (false, "reproducer_installation_undeclared"),
        (_, _, _, None) => (false, "producer_installation_undeclared"),
        (_, _, Some(ri), Some(pi)) if ri == pi => (false, "same_installation"),
        _ => (false, "incomparable"),
    }
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
