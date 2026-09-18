//! The foreign-model claim records (spec §5h.8; R-2.9.8⁰; S1.24;
//! ADR-0202/0203).
//!
//! - [`ForeignRef`] — a claim about a foreign-system digest; **never a
//!   `ContentAddress`** (I-2: imports never pin identity).
//! - [`SnapshotClaim`] — the additive extension of `ModelDiscoveryClaim` for
//!   the C0 schema slice.
//! - [`CompatibilityRecord`] — the `conformance_report{subject_kind:
//!   snapshot_pair}` record (`SubjectKind::SnapshotPair` exists in
//!   `hh-registry`; S1.24 lands the record shape).
//! - [`TrainedUnderRef`] / [`TrainingCutoffClaim`] / [`TrainingLineage`] —
//!   the claim members.
//!
//! Everything here is a *claim*: provenance is a member, never derived; a
//! foreign digest claim never resolves into the single namespace (N8).

use std::collections::BTreeMap;

use hh_ontology::debt::ExpiryCondition;
use hh_provenance::ProvenanceRecord;
use hh_wire::Json;

use crate::json_util::*;

// ── ForeignRef ──────────────────────────────────────────────────────────────

/// `ForeignRef{system, digest, label?, provenance}` — a claim about a
/// foreign-system digest (§5h.8; I-2). The digest's *claim* is recorded; it
/// is never a `ContentAddress` and never pins a `Snapshot` (imports are
/// claims-only).
#[derive(Debug, Clone, PartialEq)]
pub struct ForeignRef {
    /// The foreign system that minted the digest (`"hf"`, `"vendor"`, …).
    pub system: String,
    /// The digest string as claimed (a foreign hash encoding).
    pub digest: String,
    /// A human label for the claim.
    pub label: Option<String>,
    /// Who claims it (mandatory — a claim without provenance is not a claim).
    pub provenance: ProvenanceRecord,
}

impl ForeignRef {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("system".into(), Json::str(&self.system));
        m.insert("digest".into(), Json::str(&self.digest));
        if let Some(l) = &self.label {
            m.insert("label".into(), Json::str(l));
        }
        m.insert("provenance".into(), self.provenance.to_json());
        Json::Obj(m)
    }

    /// Strict decode — `provenance` is mandatory (R-TEXT's sibling rule: a
    /// claim record carries who claims it).
    pub fn from_json(j: &Json) -> Result<ForeignRef, SchemaError> {
        const REC: &str = "ForeignRef";
        let m = expect_obj(j, REC)?;
        reject_unknown(m, &["system", "digest", "label", "provenance"], REC)?;
        Ok(ForeignRef {
            system: str_at(m, "system", REC)?.to_string(),
            digest: str_at(m, "digest", REC)?.to_string(),
            label: opt_str_at(m, "label")?.map(str::to_string),
            provenance: ProvenanceRecord::from_json(member_at(m, "provenance", REC)?)
                .map_err(|e| SchemaError::v("provenance", format!("{e:?}")))?,
        })
    }
}

// ── TrainingCutoffClaim ─────────────────────────────────────────────────────

/// `CutoffProvenance` — who states the cutoff (§5h.8; R-2.9.4⁰ᵃ row 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CutoffProvenance {
    /// The provider stated the cutoff.
    ProviderStated,
    /// The cutoff was inferred (e.g. from training-data evidence).
    Inferred,
    /// The provenance is unknown — the weakest claim.
    Unknown,
}

impl CutoffProvenance {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            CutoffProvenance::ProviderStated => "provider_stated",
            CutoffProvenance::Inferred => "inferred",
            CutoffProvenance::Unknown => "unknown",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<CutoffProvenance> {
        match s {
            "provider_stated" => Some(CutoffProvenance::ProviderStated),
            "inferred" => Some(CutoffProvenance::Inferred),
            "unknown" => Some(CutoffProvenance::Unknown),
            _ => None,
        }
    }
}

/// `training_cutoff_claim{value?, provenance}` — the snapshot's claimed
/// training-data cutoff (§5h.4/§5h.8; R-2.9.4⁰ᵃ row 5). `value` absent with
/// `provenance = unknown` is the honest "no claim" form.
#[derive(Debug, Clone, PartialEq)]
pub struct TrainingCutoffClaim {
    /// The claimed cutoff (a date string; `None` = no cutoff claimed).
    pub value: Option<String>,
    /// Who claims it.
    pub provenance: CutoffProvenance,
}

impl TrainingCutoffClaim {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        if let Some(v) = &self.value {
            m.insert("value".into(), Json::str(v));
        }
        m.insert("provenance".into(), Json::str(self.provenance.name()));
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<TrainingCutoffClaim, SchemaError> {
        const REC: &str = "TrainingCutoffClaim";
        let m = expect_obj(j, REC)?;
        reject_unknown(m, &["value", "provenance"], REC)?;
        Ok(TrainingCutoffClaim {
            value: opt_str_at(m, "value")?.map(str::to_string),
            provenance: CutoffProvenance::parse(str_at(m, "provenance", REC)?)
                .ok_or_else(|| SchemaError::v("provenance", "unknown cutoff provenance"))?,
        })
    }
}

// ── TrainingLineage ─────────────────────────────────────────────────────────

/// `compute_claim{confidence = reported}` — a compute claim's confidence is
/// always `reported` at C0 (a claim, never a measurement).
#[derive(Debug, Clone, PartialEq)]
pub struct ComputeClaim {
    /// The claimed compute descriptor (schema-opaque at C0).
    pub descriptor: Json,
}

/// `training_lineage{training_run_ref, step, data_refs[], method,
/// compute_claim{confidence = reported}}` — the snapshot's claimed lineage
/// (§5h.8).
#[derive(Debug, Clone, PartialEq)]
pub struct TrainingLineage {
    /// The claimed training-run ref.
    pub training_run_ref: String,
    /// The claimed checkpoint step.
    pub step: u64,
    /// The claimed training-data refs.
    pub data_refs: Vec<String>,
    /// The claimed training method.
    pub method: String,
    /// The compute claim (`confidence = reported` — carried, never verified).
    pub compute_claim: ComputeClaim,
}

impl TrainingLineage {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut cc = BTreeMap::new();
        cc.insert("confidence".into(), Json::str("reported"));
        cc.insert("descriptor".into(), self.compute_claim.descriptor.clone());
        Json::obj([
            ("training_run_ref", Json::str(&self.training_run_ref)),
            ("step", Json::Int(self.step as i64)),
            (
                "data_refs",
                Json::Arr(self.data_refs.iter().map(Json::str).collect()),
            ),
            ("method", Json::str(&self.method)),
            ("compute_claim", Json::Obj(cc)),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<TrainingLineage, SchemaError> {
        const REC: &str = "TrainingLineage";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "training_run_ref",
                "step",
                "data_refs",
                "method",
                "compute_claim",
            ],
            REC,
        )?;
        let cc = expect_obj(member_at(m, "compute_claim", REC)?, "ComputeClaim")?;
        match str_at(cc, "confidence", "ComputeClaim")? {
            "reported" => {}
            other => {
                return Err(SchemaError::v(
                    "confidence",
                    format!("compute_claim.confidence must be `reported`, got `{other}`"),
                ))
            }
        }
        Ok(TrainingLineage {
            training_run_ref: str_at(m, "training_run_ref", REC)?.to_string(),
            step: int_at(m, "step", REC)? as u64,
            data_refs: str_vec_at(m, "data_refs", REC)?,
            method: str_at(m, "method", REC)?.to_string(),
            compute_claim: ComputeClaim {
                descriptor: cc.get("descriptor").cloned().unwrap_or(Json::Null),
            },
        })
    }
}

// ── TrainedUnderRef ─────────────────────────────────────────────────────────

/// `trained_under[]{definition semantic_id, profile semantic_id, bundle_id}` —
/// a claim that the snapshot was trained under a given definition/profile
/// pair produced by a given bundle (§5h.8).
#[derive(Debug, Clone, PartialEq)]
pub struct TrainedUnderRef {
    /// The definition's semantic id (aggregation coordinate).
    pub definition_semantic_id: String,
    /// The profile's semantic id.
    pub profile_semantic_id: String,
    /// The training bundle's id.
    pub bundle_id: String,
}

impl TrainedUnderRef {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "definition_semantic_id",
                Json::str(&self.definition_semantic_id),
            ),
            ("profile_semantic_id", Json::str(&self.profile_semantic_id)),
            ("bundle_id", Json::str(&self.bundle_id)),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<TrainedUnderRef, SchemaError> {
        const REC: &str = "TrainedUnderRef";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &["definition_semantic_id", "profile_semantic_id", "bundle_id"],
            REC,
        )?;
        Ok(TrainedUnderRef {
            definition_semantic_id: str_at(m, "definition_semantic_id", REC)?.to_string(),
            profile_semantic_id: str_at(m, "profile_semantic_id", REC)?.to_string(),
            bundle_id: str_at(m, "bundle_id", REC)?.to_string(),
        })
    }
}

// ── SnapshotClaim ───────────────────────────────────────────────────────────

/// `policy_version_exposed ∈ {supported, unsupported, unknown}` — whether the
/// provider exposes the model's policy/version pin (§5h.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PolicyVersionExposed {
    /// The provider exposes a policy-version pin.
    Supported,
    /// No policy-version pin is exposed.
    Unsupported,
    /// Unknown.
    Unknown,
}

impl PolicyVersionExposed {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            PolicyVersionExposed::Supported => "supported",
            PolicyVersionExposed::Unsupported => "unsupported",
            PolicyVersionExposed::Unknown => "unknown",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<PolicyVersionExposed> {
        match s {
            "supported" => Some(PolicyVersionExposed::Supported),
            "unsupported" => Some(PolicyVersionExposed::Unsupported),
            "unknown" => Some(PolicyVersionExposed::Unknown),
            _ => None,
        }
    }
}

/// `SnapshotClaim{provider, model_id, snapshot_id, serving_route?,
/// base_snapshot_ref?, training_lineage?, trained_under[]?,
/// training_cutoff_claim?, weights_digest?: ForeignRef,
/// policy_version_exposed}` — the additive extension of
/// `ModelDiscoveryClaim` (§5h.8; R-2.9.8⁰; ADR-0202).
///
/// Every member is a **claim**: `weights_digest` is a [`ForeignRef`], never a
/// `ContentAddress`; `trained_under[]` entries claim the definition/profile
/// the snapshot was trained under; `policy_version_exposed` records whether
/// the provider exposes a pin.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotClaim {
    /// The provider id.
    pub provider: String,
    /// The model id.
    pub model_id: String,
    /// The snapshot id.
    pub snapshot_id: String,
    /// The serving route, where claimed.
    pub serving_route: Option<String>,
    /// The base snapshot this derives from, where claimed.
    pub base_snapshot_ref: Option<String>,
    /// The claimed training lineage.
    pub training_lineage: Option<TrainingLineage>,
    /// The `trained_under` claims.
    pub trained_under: Vec<TrainedUnderRef>,
    /// The training-cutoff claim.
    pub training_cutoff_claim: Option<TrainingCutoffClaim>,
    /// The claimed weights digest (a [`ForeignRef`] — never a pin; I-2).
    pub weights_digest: Option<ForeignRef>,
    /// Whether the provider exposes a policy-version pin.
    pub policy_version_exposed: PolicyVersionExposed,
}

impl SnapshotClaim {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("provider".into(), Json::str(&self.provider));
        m.insert("model_id".into(), Json::str(&self.model_id));
        m.insert("snapshot_id".into(), Json::str(&self.snapshot_id));
        if let Some(r) = &self.serving_route {
            m.insert("serving_route".into(), Json::str(r));
        }
        if let Some(b) = &self.base_snapshot_ref {
            m.insert("base_snapshot_ref".into(), Json::str(b));
        }
        if let Some(t) = &self.training_lineage {
            m.insert("training_lineage".into(), t.to_json());
        }
        if !self.trained_under.is_empty() {
            m.insert(
                "trained_under".into(),
                Json::Arr(
                    self.trained_under
                        .iter()
                        .map(TrainedUnderRef::to_json)
                        .collect(),
                ),
            );
        }
        if let Some(c) = &self.training_cutoff_claim {
            m.insert("training_cutoff_claim".into(), c.to_json());
        }
        if let Some(w) = &self.weights_digest {
            m.insert("weights_digest".into(), w.to_json());
        }
        m.insert(
            "policy_version_exposed".into(),
            Json::str(self.policy_version_exposed.name()),
        );
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<SnapshotClaim, SchemaError> {
        const REC: &str = "SnapshotClaim";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "provider",
                "model_id",
                "snapshot_id",
                "serving_route",
                "base_snapshot_ref",
                "training_lineage",
                "trained_under",
                "training_cutoff_claim",
                "weights_digest",
                "policy_version_exposed",
            ],
            REC,
        )?;
        Ok(SnapshotClaim {
            provider: str_at(m, "provider", REC)?.to_string(),
            model_id: str_at(m, "model_id", REC)?.to_string(),
            snapshot_id: str_at(m, "snapshot_id", REC)?.to_string(),
            serving_route: opt_str_at(m, "serving_route")?.map(str::to_string),
            base_snapshot_ref: opt_str_at(m, "base_snapshot_ref")?.map(str::to_string),
            training_lineage: match m.get("training_lineage") {
                None | Some(Json::Null) => None,
                Some(t) => Some(TrainingLineage::from_json(t)?),
            },
            trained_under: match opt_arr_at(m, "trained_under")? {
                None => Vec::new(),
                Some(a) => a
                    .iter()
                    .map(TrainedUnderRef::from_json)
                    .collect::<Result<_, _>>()?,
            },
            training_cutoff_claim: match m.get("training_cutoff_claim") {
                None | Some(Json::Null) => None,
                Some(c) => Some(TrainingCutoffClaim::from_json(c)?),
            },
            weights_digest: match m.get("weights_digest") {
                None | Some(Json::Null) => None,
                Some(w) => Some(ForeignRef::from_json(w)?),
            },
            policy_version_exposed: PolicyVersionExposed::parse(str_at(
                m,
                "policy_version_exposed",
                REC,
            )?)
            .ok_or_else(|| SchemaError::v("policy_version_exposed", "unknown exposure value"))?,
        })
    }
}

// ── CompatibilityRecord ─────────────────────────────────────────────────────

/// `status ∈ {trained_under, verified, drifted{rules[]}, broken{report_ref},
/// unknown}` — the snapshot↔definition compatibility state (§5h.8;
/// ADR-0203).
#[derive(Debug, Clone, PartialEq)]
pub enum CompatibilityStatus {
    /// The snapshot claims it was trained under this definition/profile.
    TrainedUnder,
    /// The compatibility was verified (a conformance report exists).
    Verified,
    /// The compatibility drifted — the named rules diverge.
    Drifted {
        /// The rules that drifted.
        rules: Vec<String>,
    },
    /// The compatibility is broken — the conformance report that broke it.
    Broken {
        /// The breaking report's ref.
        report_ref: String,
    },
    /// No compatibility information.
    Unknown,
}

impl CompatibilityStatus {
    /// The canonical JSON (a one-key sum object).
    pub fn to_json(&self) -> Json {
        match self {
            CompatibilityStatus::TrainedUnder => Json::str("trained_under"),
            CompatibilityStatus::Verified => Json::str("verified"),
            CompatibilityStatus::Drifted { rules } => Json::obj([(
                "drifted",
                Json::obj([("rules", Json::Arr(rules.iter().map(Json::str).collect()))]),
            )]),
            CompatibilityStatus::Broken { report_ref } => {
                Json::obj([("broken", Json::obj([("report_ref", Json::str(report_ref))]))])
            }
            CompatibilityStatus::Unknown => Json::str("unknown"),
        }
    }

    /// Strict decode — a bare spelling or a `{variant: {…}}` object.
    pub fn from_json(j: &Json) -> Result<CompatibilityStatus, SchemaError> {
        match j {
            Json::Str(s) => match s.as_str() {
                "trained_under" => Ok(CompatibilityStatus::TrainedUnder),
                "verified" => Ok(CompatibilityStatus::Verified),
                "unknown" => Ok(CompatibilityStatus::Unknown),
                _ => Err(SchemaError::v("status", format!("unknown status `{s}`"))),
            },
            Json::Obj(m) if m.len() == 1 => {
                let (k, v) = m.iter().next().expect("len checked");
                match k.as_str() {
                    "drifted" => {
                        let dm = expect_obj(v, "status.drifted")?;
                        reject_unknown(dm, &["rules"], "status.drifted")?;
                        Ok(CompatibilityStatus::Drifted {
                            rules: str_vec_at(dm, "rules", "status.drifted")?,
                        })
                    }
                    "broken" => {
                        let bm = expect_obj(v, "status.broken")?;
                        reject_unknown(bm, &["report_ref"], "status.broken")?;
                        Ok(CompatibilityStatus::Broken {
                            report_ref: str_at(bm, "report_ref", "status.broken")?.to_string(),
                        })
                    }
                    _ => Err(SchemaError::v("status", format!("unknown status `{k}`"))),
                }
            }
            _ => Err(SchemaError::v(
                "status",
                "must be a string or one-key object",
            )),
        }
    }
}

/// `CompatibilityRecord` — the `conformance_report{subject_kind:
/// snapshot_pair}` record (§5h.8; R-2.9.8⁰; ADR-0203):
/// `{snapshot_ref, definition semantic_id, profile semantic_id, status,
/// evidence_ref?, regression_suite_ref?, created_at, expiry_condition?,
/// provenance}`.
///
/// The `subject_kind = snapshot_pair` binding is the registry's
/// (`SubjectKind::SnapshotPair` exists; S1.24 lands the record shape).
#[derive(Debug, Clone, PartialEq)]
pub struct CompatibilityRecord {
    /// The snapshot the record is about.
    pub snapshot_ref: String,
    /// The definition's semantic id.
    pub definition_semantic_id: String,
    /// The profile's semantic id.
    pub profile_semantic_id: String,
    /// The compatibility state.
    pub status: CompatibilityStatus,
    /// The evidence ref behind the status (mandatory for `verified`/`drifted`/`broken`).
    pub evidence_ref: Option<String>,
    /// The regression suite the verdict ran under.
    pub regression_suite_ref: Option<String>,
    /// The record's logical creation time (a `seq`, never a wall clock).
    pub created_at: u64,
    /// The record's expiry condition (the §5h.6 expiry vocabulary — a
    /// `verified` compatibility that drifts expires).
    pub expiry_condition: Option<ExpiryCondition>,
    /// The record's provenance (mandatory).
    pub provenance: ProvenanceRecord,
}

/// `CompatibilityRecord` schema errors.
#[derive(Debug, Clone, PartialEq)]
pub enum CompatibilityError {
    /// A member-level schema violation.
    Schema(SchemaError),
    /// `status ∈ {verified, drifted, broken}` requires `evidence_ref` — a
    /// verdict without evidence is not a verdict.
    MissingEvidence,
}

impl From<SchemaError> for CompatibilityError {
    fn from(e: SchemaError) -> CompatibilityError {
        CompatibilityError::Schema(e)
    }
}

impl CompatibilityRecord {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("snapshot_ref".into(), Json::str(&self.snapshot_ref));
        m.insert(
            "definition_semantic_id".into(),
            Json::str(&self.definition_semantic_id),
        );
        m.insert(
            "profile_semantic_id".into(),
            Json::str(&self.profile_semantic_id),
        );
        m.insert("status".into(), self.status.to_json());
        if let Some(e) = &self.evidence_ref {
            m.insert("evidence_ref".into(), Json::str(e));
        }
        if let Some(r) = &self.regression_suite_ref {
            m.insert("regression_suite_ref".into(), Json::str(r));
        }
        m.insert("created_at".into(), Json::Int(self.created_at as i64));
        if let Some(x) = &self.expiry_condition {
            m.insert("expiry_condition".into(), x.to_json());
        }
        m.insert("provenance".into(), self.provenance.to_json());
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<CompatibilityRecord, CompatibilityError> {
        const REC: &str = "CompatibilityRecord";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "snapshot_ref",
                "definition_semantic_id",
                "profile_semantic_id",
                "status",
                "evidence_ref",
                "regression_suite_ref",
                "created_at",
                "expiry_condition",
                "provenance",
            ],
            REC,
        )?;
        Ok(CompatibilityRecord {
            snapshot_ref: str_at(m, "snapshot_ref", REC)?.to_string(),
            definition_semantic_id: str_at(m, "definition_semantic_id", REC)?.to_string(),
            profile_semantic_id: str_at(m, "profile_semantic_id", REC)?.to_string(),
            status: CompatibilityStatus::from_json(member_at(m, "status", REC)?)?,
            evidence_ref: opt_str_at(m, "evidence_ref")?.map(str::to_string),
            regression_suite_ref: opt_str_at(m, "regression_suite_ref")?.map(str::to_string),
            created_at: int_at(m, "created_at", REC)? as u64,
            expiry_condition: match m.get("expiry_condition") {
                None | Some(Json::Null) => None,
                Some(x) => Some(
                    ExpiryCondition::from_json(x, "expiry_condition")
                        .map_err(|e| SchemaError::v("expiry_condition", format!("{e:?}")))?,
                ),
            },
            provenance: ProvenanceRecord::from_json(member_at(m, "provenance", REC)?)
                .map_err(|e| SchemaError::v("provenance", format!("{e:?}")))?,
        })
    }

    /// The schema check: a `verified`/`drifted`/`broken` status carries
    /// `evidence_ref` (the claim–evidence partition; §5h.8).
    pub fn validate(&self) -> Result<(), CompatibilityError> {
        match &self.status {
            CompatibilityStatus::Verified
            | CompatibilityStatus::Drifted { .. }
            | CompatibilityStatus::Broken { .. }
                if self.evidence_ref.is_none() =>
            {
                Err(CompatibilityError::MissingEvidence)
            }
            _ => Ok(()),
        }
    }
}
