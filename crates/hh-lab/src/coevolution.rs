//! The §5h.8 model↔harness **co-evolution interface** (R-2.9.8; S6.4;
//! ADR-0325): the external-training boundary, the snapshot
//! import/bind-guard machinery, consolidation over the assumption-debt
//! plane, and the sidecar `CoEvolutionCycleRecord`.
//!
//! The boundary invariants (§5h.8 §1 N9):
//!
//! - HarnessHarness **never trains, loads, merges, quantizes, or serves
//!   weights** — training is a foreign participant reachable only through
//!   the `training_export/1` lowering target and the `import_snapshot`
//!   claim path. No crate stores checkpoints, datasets, or optimizer
//!   state.
//! - [`project_training_export`] is a *pure projection* of the ledger prefix
//!   (records-in/records-out — the caller supplies decoded ledger
//!   envelopes as canonical `Json` rows, this crate holds no `hh-ledger`
//!   edge). Same prefix + same policy ⇒ the byte-identical export (E-1).
//! - [`import_snapshot`] takes a [`SnapshotClaim`] — claims-only, never
//!   identity-pinning (I-2) — and produces the `ModelSnapshotRecord` +
//!   the `trained_under`/`unknown` [`CompatibilityRecord`] set.
//! - Consolidation is a *proposal* — never an applied diff (R-2.9.5) —
//!   and retires only through the §5h.6 retirement experiment + human
//!   seal (R-2.9.6 D5/D7).
//! - `CoEvolutionCycleRecord`/`CyclePolicy` are **sidecar records** over
//!   ordinary experiments — the cycle introduces no new `run_kind`,
//!   registry kind, design kind, or event family (each phase's evidence
//!   is the same `ComparisonReport` vocabulary).
//!
//! Every operation is `research-grade`; every output is `preview`-
//! labelled and walled off from C0–C3 acceptance and leaderboards
//! ([`MATURITY`]/[`PREVIEW_LABEL`]; §5h.8 maturity row).

use std::collections::BTreeMap;

use hh_ontology::debt::{
    DebtClass, DebtExpiry, DebtScope, DebtStatus, DeficiencyClass, EvidenceKind, EvidenceRef,
    ExpiryCondition, ExpiryKind, ExpiryParams, HypothesisSubject, HypothesisTyped, ModelSelector,
    OwnerRef, PredictedEffect, RemovalTest, RemovalTestKind, Revalidation, RevalidationAction,
    RevalidationOn,
};
use hh_provenance::ProvenanceRecord;
use hh_wire::Json;

use crate::json_util::*;
use crate::model::{
    CompatibilityRecord, CompatibilityStatus, PolicyVersionExposed, SnapshotClaim, TrainedUnderRef,
    TrainingLineage,
};

/// The maturity every S6.4 operation and output carries (§5h.8 maturity
/// row — a lower maturity than the interface's constituents).
pub const MATURITY: &str = "research-grade";

/// The report label walling every output off headline results and C0–C3
/// acceptance (R-2.9.8; `preview` rows never ride `headline`).
pub const PREVIEW_LABEL: &str = "preview";

/// The `training_export` dialect tag.
pub const TRAINING_EXPORT_DIALECT: &str = "training_export/1";

// ── Errors — the §5h.8 closed refusal set ───────────────────────────────────

/// The co-evolution boundary's closed refusal sum (§5h.8 §3; every member
/// is a refusal, never a warning — E-0).
#[derive(Debug, Clone, PartialEq)]
pub enum CoEvolutionError {
    /// E-2 — the export would carry a `held_out`/`private`-labelled task
    /// (the `task_splits` projection names the offending task).
    HeldOutInExport {
        /// The held-out task ref.
        task_ref: String,
    },
    /// E-4 — the policy declares a reader the corpus record does not
    /// admit (the reader list is the corpus's declared reader set's
    /// subset, never a superset).
    ReaderViolation {
        /// The offending reader.
        reader: String,
    },
    /// E-3/E-3a — a reward row names a source the admissible set refuses
    /// (`critic`/`reward_model`/`participant_reported` verdicts are never
    /// rewards; an undeclared source is refused, never silently dropped).
    InadmissibleRewardSource {
        /// The offending source/oracle class.
        source: String,
    },
    /// I-0 — the `SnapshotClaim` fails member-level validation (no
    /// provenance, empty `snapshot_id`, …).
    ClaimSchemaInvalid {
        /// The failure detail.
        detail: String,
    },
    /// I-4 — the claim's `base_snapshot_ref` names a snapshot the import
    /// context does not know.
    BaseSnapshotUnknown {
        /// The unresolved base ref.
        base_ref: String,
    },
    /// I-4a — the claim carries a `serving_route` the caller reports
    /// unreachable (`unchecked` is admissible; a reported unreachable
    /// route refuses).
    ServingRouteUnreachable {
        /// The unreachable route.
        route: String,
    },
    /// I-4b — the `trained_under` lineage would cycle (the new snapshot
    /// already descends from a snapshot that names it).
    LineageCycle {
        /// The cycling snapshot id.
        snapshot_id: String,
    },
    /// G-2 — the arm binds a snapshot whose `CompatibilityRecord` is
    /// `broken` (evidence-backed refusal; the report ref rides the code).
    CompatibilityBroken {
        /// The breaking report's ref.
        report_ref: String,
    },
    /// G-3 — an evolution-origin arm binds a snapshot with **no**
    /// `CompatibilityRecord` and neither the exploratory declaration nor
    /// `allow_unverified_snapshot` lifts the refusal (§5h.8 §3).
    ExploratoryOnly {
        /// The failure detail.
        detail: String,
    },
    /// The export's `UnexpressibleMember` row escalates to a refusal —
    /// a member the target shape cannot carry and no loss class admits.
    UnexpressibleMember {
        /// The member/detail.
        detail: String,
    },
    /// The source bundle is not `valid` for export (E-0; a missing ledger
    /// member, a `status ∉ {present}` page the projection needs).
    BundleNotValid {
        /// The offending member/detail.
        detail: String,
    },
    /// A consolidation verdict cites no target `ComparisonReport` — the
    /// `absorbed` verdict never lands without the target's own evidence
    /// (§5h.8 §5.1 V1).
    MissingComparisonReport {
        /// Which report is absent.
        which: String,
    },
    /// The cycle's switch rule / phase kind is outside the declared
    /// policy (a closed-set violation, never a silent default).
    IllegalPhase {
        /// The failure detail.
        detail: String,
    },
    /// Member-level schema failure.
    Schema(SchemaError),
}

impl CoEvolutionError {
    /// The closed refusal code (the `Refused{reason}` spelling).
    pub fn code(&self) -> String {
        match self {
            CoEvolutionError::HeldOutInExport { .. } => "HeldOutInExport".to_string(),
            CoEvolutionError::ReaderViolation { .. } => "ReaderViolation".to_string(),
            CoEvolutionError::InadmissibleRewardSource { .. } => {
                "InadmissibleRewardSource".to_string()
            }
            CoEvolutionError::ClaimSchemaInvalid { .. } => "ClaimSchemaInvalid".to_string(),
            CoEvolutionError::BaseSnapshotUnknown { .. } => "BaseSnapshotUnknown".to_string(),
            CoEvolutionError::ServingRouteUnreachable { .. } => {
                "ServingRouteUnreachable".to_string()
            }
            CoEvolutionError::LineageCycle { .. } => "LineageCycle".to_string(),
            CoEvolutionError::CompatibilityBroken { .. } => "CompatibilityBroken".to_string(),
            CoEvolutionError::ExploratoryOnly { .. } => "ExploratoryOnly".to_string(),
            CoEvolutionError::UnexpressibleMember { .. } => "UnexpressibleMember".to_string(),
            CoEvolutionError::BundleNotValid { .. } => "BundleNotValid".to_string(),
            CoEvolutionError::MissingComparisonReport { .. } => {
                "MissingComparisonReport".to_string()
            }
            CoEvolutionError::IllegalPhase { .. } => "IllegalPhase".to_string(),
            CoEvolutionError::Schema(e) => format!("SchemaViolation:{:?}", e.detail),
        }
    }
}

impl From<SchemaError> for CoEvolutionError {
    fn from(e: SchemaError) -> CoEvolutionError {
        CoEvolutionError::Schema(e)
    }
}

// ── TrainingExportPolicy ────────────────────────────────────────────────────

/// `sample_unit ∈ {turn, step, tool_call}` — the export's sample
/// granularity (§5h.8 §2.1; closed at export time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleUnit {
    /// One sample per `model.call.completed` row.
    Turn,
    /// One sample per `model.call.attempt.completed` row.
    Step,
    /// One sample per `tool.call.completed` row.
    ToolCall,
}

impl SampleUnit {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            SampleUnit::Turn => "turn",
            SampleUnit::Step => "step",
            SampleUnit::ToolCall => "tool_call",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<SampleUnit> {
        match s {
            "turn" => Some(SampleUnit::Turn),
            "step" => Some(SampleUnit::Step),
            "tool_call" => Some(SampleUnit::ToolCall),
            _ => None,
        }
    }

    /// The ledger event class the unit projects.
    fn event_class(self) -> &'static str {
        match self {
            SampleUnit::Turn => "model.call.completed",
            SampleUnit::Step => "model.call.attempt.completed",
            SampleUnit::ToolCall => "tool.call.completed",
        }
    }
}

/// `TrainingExportPolicy{sample_unit, include[], readers[], redaction?,
/// reward_sources[], split_labels[], snapshot_filter?}` (§5h.8 §2.1).
///
/// - `include[]` — the member classes projected, closed:
///   `{model_call, tool_call, reward, trajectory, harness_constraints}`.
/// - `readers[]` — the declared readers; E-4 gates them against the
///   corpus's declared reader set (a subset, never a superset).
/// - `reward_sources[]` — the admissible reward `source` spellings this
///   export admits (a subset of [`ADMISSIBLE_REWARD_SOURCES`]; an event
///   naming anything else is `InadmissibleRewardSource`, never dropped).
/// - `split_labels[]` — the split labels the export may carry;
///   `held_out`/`private` are *not* admissible here (they refuse at
///   projection — `HeldOutInExport`).
/// - `snapshot_filter` — confine samples to one `snapshot_id`.
#[derive(Debug, Clone, PartialEq)]
pub struct TrainingExportPolicy {
    /// The sample granularity.
    pub sample_unit: SampleUnit,
    /// The member classes projected.
    pub include: Vec<String>,
    /// The declared readers.
    pub readers: Vec<String>,
    /// The redaction recipe ref (`policy.redaction` — applied before the
    /// reward attaches; redactions list on the `RewardRecord`).
    pub redaction: Option<String>,
    /// The admissible reward `source` spellings.
    pub reward_sources: Vec<String>,
    /// The split labels the export may carry.
    pub split_labels: Vec<String>,
    /// The `snapshot_id` the export confines to (`None` = all).
    pub snapshot_filter: Option<String>,
}

/// The include classes the projection admits (E-1/E-7 — a member outside
/// this set is `UnexpressibleMember`, never silently dropped).
pub const ADMISSIBLE_INCLUDES: &[&str] = &[
    "model_call",
    "tool_call",
    "reward",
    "trajectory",
    "harness_constraints",
];

/// The reward `source` spellings the admissible set admits (§5h.8 §2.1
/// E-3: oracle rewards only — critic verdicts, reward-model outputs and
/// participant-reported rows are *never* rewards).
pub const ADMISSIBLE_REWARD_SOURCES: &[&str] = &["oracle_metric", "oracle_verdict", "metric_value"];

/// The reward `source` spellings that refuse outright (E-3a — the
/// closed "never a reward" set).
pub const REFUSED_REWARD_SOURCES: &[&str] = &[
    "critic_verdict",
    "reward_model_output",
    "participant_reported",
];

/// The split labels that refuse in any export (E-2 — held-out/private
/// surfaces never leave the ledger).
pub const REFUSED_SPLIT_LABELS: &[&str] = &["held_out", "private"];

/// The event classes the reward projection reads (`payload.reward`
/// carrying `{source, oracle_ref, oracle_class, value?, unit?}`).
const REWARD_EVENT_CLASSES: &[&str] = &[
    "measurement.metric.emitted",
    "measurement.oracle.metric.emitted",
    "measurement.oracle.verdict",
    "metric.emitted",
    "oracle.verdict",
];

impl TrainingExportPolicy {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("sample_unit".into(), Json::str(self.sample_unit.name()));
        m.insert(
            "include".into(),
            Json::Arr(self.include.iter().map(Json::str).collect()),
        );
        m.insert(
            "readers".into(),
            Json::Arr(self.readers.iter().map(Json::str).collect()),
        );
        if let Some(r) = &self.redaction {
            m.insert("redaction".into(), Json::str(r));
        }
        m.insert(
            "reward_sources".into(),
            Json::Arr(self.reward_sources.iter().map(Json::str).collect()),
        );
        m.insert(
            "split_labels".into(),
            Json::Arr(self.split_labels.iter().map(Json::str).collect()),
        );
        if let Some(f) = &self.snapshot_filter {
            m.insert("snapshot_filter".into(), Json::str(f));
        }
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<TrainingExportPolicy, CoEvolutionError> {
        const REC: &str = "TrainingExportPolicy";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "sample_unit",
                "include",
                "readers",
                "redaction",
                "reward_sources",
                "split_labels",
                "snapshot_filter",
            ],
            REC,
        )?;
        Ok(TrainingExportPolicy {
            sample_unit: SampleUnit::parse(str_at(m, "sample_unit", REC)?)
                .ok_or_else(|| SchemaError::v("sample_unit", "unknown sample_unit"))?,
            include: str_vec_at(m, "include", REC)?,
            readers: str_vec_at(m, "readers", REC)?,
            redaction: opt_str_at(m, "redaction")?.map(str::to_string),
            reward_sources: str_vec_at(m, "reward_sources", REC)?,
            split_labels: str_vec_at(m, "split_labels", REC)?,
            snapshot_filter: opt_str_at(m, "snapshot_filter")?.map(str::to_string),
        })
    }

    /// The member-level check (closed-set legs; the context legs —
    /// readers ⊆ corpus readers — run in [`project_training_export`]).
    pub fn validate(&self) -> Result<(), CoEvolutionError> {
        for i in &self.include {
            if !ADMISSIBLE_INCLUDES.contains(&i.as_str()) {
                return Err(CoEvolutionError::UnexpressibleMember {
                    detail: format!("include `{i}` is outside the training_export member set"),
                });
            }
        }
        for s in &self.reward_sources {
            if !ADMISSIBLE_REWARD_SOURCES.contains(&s.as_str()) {
                return Err(CoEvolutionError::InadmissibleRewardSource { source: s.clone() });
            }
        }
        for l in &self.split_labels {
            if REFUSED_SPLIT_LABELS.contains(&l.as_str()) {
                return Err(CoEvolutionError::HeldOutInExport {
                    task_ref: format!("split_label:{l}"),
                });
            }
        }
        Ok(())
    }
}

// ── RewardRecord / TrainingSample / TrainingExposureRecord ─────────────────

/// `RewardRecord{value?, unit?, oracle_ref, oracle_class, source,
/// provenance, redaction_applied[]}` — an admissible oracle reward
/// (§5h.8 §2.1). `value` is typed `n/a`-able: an admissible-source row
/// whose value the export cannot carry attaches `{value: "n/a"}` and a
/// `narrowed` loss entry — never fabricated (E-6).
#[derive(Debug, Clone, PartialEq)]
pub struct RewardRecord {
    /// The reward's source spelling (a [`ADMISSIBLE_REWARD_SOURCES`]
    /// member — `policy.reward_sources` admits it).
    pub source: String,
    /// The oracle/producer ref the reward came from.
    pub oracle_ref: String,
    /// The oracle's class (`oracle|metric` — never `critic`).
    pub oracle_class: String,
    /// The numeric value (`None` = the export narrows it to `n/a`).
    pub value: Option<Json>,
    /// The unit spelling, where the row declares one.
    pub unit: Option<String>,
    /// The reward row's provenance (the `measurement.oracle.*` envelope's
    /// `provenance` member, verbatim).
    pub provenance: Option<Json>,
    /// `veto_tripped` — explicit, never absent (a vetoed verdict is
    /// typed, never dropped).
    pub veto_tripped: bool,
    /// The redactions the `policy.redaction` recipe applied.
    pub redaction_applied: Vec<String>,
}

impl RewardRecord {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("source".into(), Json::str(&self.source));
        m.insert("oracle_ref".into(), Json::str(&self.oracle_ref));
        m.insert("oracle_class".into(), Json::str(&self.oracle_class));
        m.insert(
            "value".into(),
            self.value.clone().unwrap_or(Json::str("n/a")),
        );
        if let Some(u) = &self.unit {
            m.insert("unit".into(), Json::str(u));
        }
        if let Some(p) = &self.provenance {
            m.insert("provenance".into(), p.clone());
        }
        m.insert("veto_tripped".into(), Json::Bool(self.veto_tripped));
        if !self.redaction_applied.is_empty() {
            m.insert(
                "redaction_applied".into(),
                Json::Arr(self.redaction_applied.iter().map(Json::str).collect()),
            );
        }
        Json::Obj(m)
    }
}

/// `TrainingSample{task_ref, input_digest, trajectory_ref, reward?,
/// veto_tripped, harness_constraints, provenance}` — one projected
/// sample (§5h.8 §2.1). `harness_constraints` is the *typed* member —
/// the sealed definition/bundle's declared constraints the trainer must
/// respect, never a prose string (E-6).
#[derive(Debug, Clone, PartialEq)]
pub struct TrainingSample {
    /// The task the sample's run names (`scope.task_id`/`payload.task_*`
    /// — `n/a` when the ledger carries none).
    pub task_ref: String,
    /// The request/input content digest (the payload's
    /// `request_plan_hash`/`input_digest` — a claim, not the bytes).
    pub input_digest: String,
    /// The run/trajectory ref the sample derives from.
    pub trajectory_ref: String,
    /// The ledger seq the sample projected from (the rebuild anchor —
    /// E-1's "same prefix ⇒ same export").
    pub seq: u64,
    /// The admissible reward, where one attached.
    pub reward: Option<RewardRecord>,
    /// `veto_tripped` — explicit at sample level (a sample over a vetoed
    /// row types it; nothing is silently dropped).
    pub veto_tripped: bool,
    /// The typed harness constraints member.
    pub harness_constraints: Json,
    /// The sample's provenance (the source envelope's).
    pub provenance: Option<Json>,
}

impl TrainingSample {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("task_ref".into(), Json::str(&self.task_ref));
        m.insert("input_digest".into(), Json::str(&self.input_digest));
        m.insert("trajectory_ref".into(), Json::str(&self.trajectory_ref));
        m.insert("seq".into(), Json::Int(self.seq as i64));
        if let Some(r) = &self.reward {
            m.insert("reward".into(), r.to_json());
        }
        m.insert("veto_tripped".into(), Json::Bool(self.veto_tripped));
        m.insert(
            "harness_constraints".into(),
            self.harness_constraints.clone(),
        );
        if let Some(p) = &self.provenance {
            m.insert("provenance".into(), p.clone());
        }
        Json::Obj(m)
    }
}

/// `TrainingExposureRecord{export_id, data_refs[], reward_provenance[],
/// provenance}` — the exposure sidecar linking the export's inputs to
/// the import's `TrainingLineage.data_refs` (§5h.8 §2.1 E-8).
#[derive(Debug, Clone, PartialEq)]
pub struct TrainingExposureRecord {
    /// The `TrainingExport`'s content id.
    pub export_id: String,
    /// The event refs the export consumed (the `data_refs` an honest
    /// trainer cites in `TrainingLineage.data_refs`).
    pub data_refs: Vec<String>,
    /// The reward rows' provenance digests.
    pub reward_provenance: Vec<String>,
    /// The exporting service's provenance.
    pub provenance: Option<Json>,
}

impl TrainingExposureRecord {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("export_id".into(), Json::str(&self.export_id));
        m.insert(
            "data_refs".into(),
            Json::Arr(self.data_refs.iter().map(Json::str).collect()),
        );
        m.insert(
            "reward_provenance".into(),
            Json::Arr(self.reward_provenance.iter().map(Json::str).collect()),
        );
        if let Some(p) = &self.provenance {
            m.insert("provenance".into(), p.clone());
        }
        Json::Obj(m)
    }
}

// ── ExportLossEntry / TrainingExport ────────────────────────────────────────

/// One loss-report entry — the same `{member?, class, detail}` shape the
/// `LoweringLossReport` uses (E-5/E-7: every unavailable, narrowed, or
/// truncated datum is a typed row; nothing fabricates token-level data).
#[derive(Debug, Clone, PartialEq)]
pub struct ExportLossEntry {
    /// The member the entry concerns (`None` = the export as a whole).
    pub member: Option<String>,
    /// The closed class — `no_slot | narrowed | truncated | redacted |
    /// unexpressible | unbudgeted`.
    pub class: String,
    /// The detail string.
    pub detail: String,
}

impl ExportLossEntry {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        if let Some(mem) = &self.member {
            m.insert("member".into(), Json::str(mem));
        }
        m.insert("class".into(), Json::str(&self.class));
        m.insert("detail".into(), Json::str(&self.detail));
        Json::Obj(m)
    }
}

/// `TrainingExport{training_export: "1", export_id, policy, samples[],
/// exposure, loss[], harness_constraints, maturity, provenance}` — the
/// `training_export/1` record (§5h.8 §2.1).
#[derive(Debug, Clone, PartialEq)]
pub struct TrainingExport {
    /// The content id (`idp/1` over the canonical body — filled on
    /// [`TrainingExport::seal`]).
    pub export_id: String,
    /// The policy the export ran under.
    pub policy: TrainingExportPolicy,
    /// The projected samples (canonical order — by `seq`).
    pub samples: Vec<TrainingSample>,
    /// The exposure sidecar.
    pub exposure: TrainingExposureRecord,
    /// The typed loss entries.
    pub loss: Vec<ExportLossEntry>,
    /// The typed harness-constraints member (the sealed definition +
    /// compiled bundle's declared constraint vector).
    pub harness_constraints: Json,
    /// The exporting service's provenance.
    pub provenance: Option<Json>,
}

impl TrainingExport {
    /// The canonical JSON (with the `training_export` dialect member and
    /// `maturity` stamp).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("training_export".into(), Json::str(TRAINING_EXPORT_DIALECT));
        m.insert("export_id".into(), Json::str(&self.export_id));
        m.insert("policy".into(), self.policy.to_json());
        m.insert(
            "samples".into(),
            Json::Arr(self.samples.iter().map(TrainingSample::to_json).collect()),
        );
        m.insert("exposure".into(), self.exposure.to_json());
        if !self.loss.is_empty() {
            m.insert(
                "loss".into(),
                Json::Arr(self.loss.iter().map(ExportLossEntry::to_json).collect()),
            );
        }
        m.insert(
            "harness_constraints".into(),
            self.harness_constraints.clone(),
        );
        m.insert("maturity".into(), Json::str(MATURITY));
        if let Some(p) = &self.provenance {
            m.insert("provenance".into(), p.clone());
        }
        Json::Obj(m)
    }

    /// Fill `export_id`/`exposure.export_id` with the `idp/1` content id
    /// of the canonical body (the export's self-address — E-1's
    /// determinism anchor).
    pub fn seal(&mut self) {
        self.export_id = String::new();
        self.exposure.export_id = String::new();
        let id = hh_identity::idp_id(
            "hh.training_export",
            &self.to_json().to_canonical_string().into_bytes(),
        );
        self.export_id = id.clone();
        self.exposure.export_id = id;
    }
}

// ── export_training — the pure projection (E-1..E-8) ───────────────────────

/// The caller-side context `export_training` reads (records-in — the
/// projection itself holds no store edge):
/// - `task_splits` — `task_ref → split_label` (E-2's held-out check).
/// - `corpus_readers` — the corpus's declared reader set (E-4).
/// - `harness_constraints` — the sealed definition/compiled bundle's
///   typed constraint vector every sample + the export carries.
#[derive(Debug, Clone)]
pub struct TrainingExportCtx {
    /// `task_ref → split_label` (the `SplitAssignmentRecord` projection).
    pub task_splits: BTreeMap<String, String>,
    /// The corpus's declared readers.
    pub corpus_readers: Vec<String>,
    /// The typed harness-constraints member.
    pub harness_constraints: Json,
    /// The exporting service's provenance stamp.
    pub provenance: Option<Json>,
}

impl Default for TrainingExportCtx {
    fn default() -> TrainingExportCtx {
        TrainingExportCtx {
            task_splits: BTreeMap::new(),
            corpus_readers: Vec::new(),
            harness_constraints: Json::obj([]),
            provenance: None,
        }
    }
}

/// The task ref an envelope names — `scope.task_id` first, then the
/// payload's `task_ref`/`task_id` (`n/a` when the ledger carries none —
/// typed, never fabricated).
fn task_ref_of(e: &Json) -> String {
    e.get("scope")
        .and_then(|s| s.get("task_id"))
        .and_then(Json::as_str)
        .or_else(|| {
            e.get("payload")
                .and_then(|p| p.get("task_ref").or_else(|| p.get("task_id")))
                .and_then(Json::as_str)
        })
        .unwrap_or("n/a")
        .to_string()
}

/// `Json::Bool` accessor (the wire `Json` carries no `as_bool`).
fn jbool(j: Option<&Json>) -> bool {
    matches!(j, Some(Json::Bool(true)))
}

/// The event's identity ref (the `data_refs` entry + `trajectory_ref`
/// anchor).
fn event_ref_of(e: &Json) -> String {
    e.get("event_id")
        .and_then(Json::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("seq:{}", e.get("seq").and_then(Json::as_int).unwrap_or(0)))
}

/// `project_training_export(run_id, events, policy, ctx) →
/// (TrainingExport, loss)` — the E-1..E-8 pure projection over one
/// subject run's decoded ledger envelopes (canonical `Json` rows — this
/// crate holds no `hh-ledger` edge).
///
/// - **E-1** — deterministic: `samples` order by `seq`; same inputs ⇒
///   the byte-identical export.
/// - **E-2** — a `task_splits` row labelling any consumed task
///   `held_out`/`private` refuses `HeldOutInExport` before a byte is
///   projected.
/// - **E-3/E-3a** — a reward row whose `source` is outside
///   `policy.reward_sources` (or in [`REFUSED_REWARD_SOURCES`]) refuses
///   `InadmissibleRewardSource`; admissible rows attach `RewardRecord`s.
/// - **E-4** — `policy.readers ⊄ ctx.corpus_readers` refuses
///   `ReaderViolation`.
/// - **E-5** — every unavailable/narrowed/truncated/redacted member is a
///   typed [`ExportLossEntry`]; `veto_tripped` is explicit on the reward
///   and the sample.
/// - **E-6** — nothing fabricates token-level data: the projection
///   carries *digests and refs*, never response bodies.
pub fn project_training_export(
    run_id: &str,
    events: &[Json],
    policy: &TrainingExportPolicy,
    ctx: &TrainingExportCtx,
) -> Result<(TrainingExport, Vec<ExportLossEntry>), CoEvolutionError> {
    policy.validate()?;
    // E-4 — reader gating against the corpus's declared set.
    for r in &policy.readers {
        if !ctx.corpus_readers.iter().any(|c| c == r) {
            return Err(CoEvolutionError::ReaderViolation { reader: r.clone() });
        }
    }
    let mut samples = Vec::new();
    let mut loss = Vec::new();
    let mut data_refs = Vec::new();
    let mut reward_prov = Vec::new();
    let mut rewards: Vec<(u64, RewardRecord)> = Vec::new();
    for e in events {
        let class = e.get("class").and_then(Json::as_str).unwrap_or_default();
        let seq = e.get("seq").and_then(Json::as_int).unwrap_or(0) as u64;
        let task = task_ref_of(e);
        // E-2 — the held-out check is per consumed task (samples and
        // rewards alike; a reward row over a held-out task refuses too).
        if let Some(label) = ctx.task_splits.get(&task) {
            if REFUSED_SPLIT_LABELS.contains(&label.as_str()) {
                return Err(CoEvolutionError::HeldOutInExport { task_ref: task });
            }
        }
        if REWARD_EVENT_CLASSES.contains(&class) {
            let reward = e
                .get("payload")
                .and_then(|p| p.get("reward").or_else(|| p.get("reward_record")))
                .cloned()
                .unwrap_or(Json::obj([]));
            let source = reward
                .get("source")
                .and_then(Json::as_str)
                .or_else(|| {
                    e.get("payload")
                        .and_then(|p| p.get("reward_source"))
                        .and_then(Json::as_str)
                })
                .unwrap_or_default()
                .to_string();
            // E-3/E-3a — never a silent drop: refused spellings and
            // undeclared sources both refuse.
            if REFUSED_REWARD_SOURCES.contains(&source.as_str())
                || !policy.reward_sources.iter().any(|s| s == &source)
            {
                return Err(CoEvolutionError::InadmissibleRewardSource { source });
            }
            let rec = RewardRecord {
                source,
                oracle_ref: reward
                    .get("oracle_ref")
                    .and_then(Json::as_str)
                    .or_else(|| {
                        e.get("payload")
                            .and_then(|p| p.get("oracle_ref"))
                            .and_then(Json::as_str)
                    })
                    .unwrap_or("n/a")
                    .to_string(),
                oracle_class: reward
                    .get("oracle_class")
                    .and_then(Json::as_str)
                    .unwrap_or("oracle")
                    .to_string(),
                value: reward.get("value").cloned(),
                unit: reward
                    .get("unit")
                    .and_then(Json::as_str)
                    .map(str::to_string),
                provenance: e.get("provenance").cloned(),
                veto_tripped: jbool(reward.get("veto_tripped"))
                    || jbool(e.get("payload").and_then(|p| p.get("veto_tripped"))),
                redaction_applied: match (&policy.redaction, reward.get("redaction_applied")) {
                    (Some(recipe), Some(Json::Arr(rs))) => rs
                        .iter()
                        .filter_map(Json::as_str)
                        .map(str::to_string)
                        .chain(std::iter::once(recipe.clone()))
                        .collect(),
                    (Some(recipe), _) => vec![recipe.clone()],
                    _ => Vec::new(),
                },
            };
            if rec.value.is_none() {
                loss.push(ExportLossEntry {
                    member: Some(format!("reward:{}", event_ref_of(e))),
                    class: "narrowed".into(),
                    detail: "admissible reward carries no expressible value — typed n/a".into(),
                });
            }
            rewards.push((seq, rec));
            data_refs.push(event_ref_of(e));
            continue;
        }
        if class != policy.sample_unit.event_class() {
            continue;
        }
        // The snapshot filter confines samples to one snapshot.
        if let Some(f) = &policy.snapshot_filter {
            let snap = e
                .get("payload")
                .and_then(|p| p.get("snapshot_id"))
                .and_then(Json::as_str)
                .or_else(|| {
                    e.get("scope")
                        .and_then(|s| s.get("snapshot_id"))
                        .and_then(Json::as_str)
                });
            if snap != Some(f.as_str()) {
                continue;
            }
        }
        let payload = e.get("payload").cloned().unwrap_or(Json::obj([]));
        let input_digest = payload
            .get("request_plan_hash")
            .or_else(|| payload.get("input_digest"))
            .and_then(Json::as_str)
            .unwrap_or("n/a")
            .to_string();
        let mut sample_losses = Vec::new();
        if input_digest == "n/a" {
            sample_losses.push(ExportLossEntry {
                member: Some(format!("sample:{}", event_ref_of(e))),
                class: "no_slot".into(),
                detail: "model-call row carries no input digest — typed n/a".into(),
            });
        }
        samples.push(TrainingSample {
            task_ref: task,
            input_digest,
            trajectory_ref: format!("{run_id}@{}", seq),
            seq,
            reward: None,
            veto_tripped: jbool(payload.get("veto_tripped")),
            harness_constraints: ctx.harness_constraints.clone(),
            provenance: e.get("provenance").cloned(),
        });
        loss.extend(sample_losses);
        data_refs.push(event_ref_of(e));
    }
    // Samples are seq-ordered by construction; attach rewards by the
    // latest sample they follow (the reward row's `sample_ref`/`for_seq`
    // member binds exactly when present).
    let mut export = TrainingExport {
        export_id: String::new(),
        policy: policy.clone(),
        samples,
        exposure: TrainingExposureRecord {
            export_id: String::new(),
            data_refs,
            reward_provenance: Vec::new(),
            provenance: ctx.provenance.clone(),
        },
        loss,
        harness_constraints: ctx.harness_constraints.clone(),
        provenance: ctx.provenance.clone(),
    };
    for (rseq, rec) in rewards {
        let bound = rec
            .provenance
            .as_ref()
            .and_then(|p| p.get("sample_ref"))
            .and_then(Json::as_str)
            .map(str::to_string);
        reward_prov.push(format!("{}:{}", rec.source, rec.oracle_ref));
        let idx = bound
            .and_then(|b| export.samples.iter().position(|s| s.trajectory_ref == b))
            .or_else(|| export.samples.iter().rposition(|s| s.seq <= rseq));
        match idx {
            Some(i) => export.samples[i].reward = Some(rec),
            None => {
                // A reward with no preceding sample is typed, never dropped.
                export.loss.push(ExportLossEntry {
                    member: Some(format!("reward@seq:{rseq}")),
                    class: "truncated".into(),
                    detail: "admissible reward binds no sample — typed in loss".into(),
                });
            }
        }
    }
    export.exposure.reward_provenance = reward_prov;
    export.seal();
    let loss = std::mem::take(&mut export.loss);
    export.loss = loss.clone();
    Ok((export, loss))
}

// ── export_regression_suite — the HarnessRegressionSuite/1 export ─────────

/// `RegressionSuiteExport{suite, unbound_level_id, spec,
/// retention_set_ref, compliance_rules[], margins_ref}` — the exported
/// `HarnessRegressionSuite` (§5h.8 §2.1): a pre-registered
/// `ExperimentSpec{kind: comparative, design: paired}` whose varied
/// factor is `model_snapshot ∈ {snapshot_in, snapshot_out(unbound)}`,
/// arms under `MatchSpec{matched_cap, cold_start}`, plus the retention
/// set and the per-rule compliance metrics. The `snapshot_out` level is
/// *unbound* — the import binds it at conformance time.
#[derive(Debug, Clone, PartialEq)]
pub struct RegressionSuiteExport {
    /// The suite id (`lab/regression/<definition>@<snapshot_in>`).
    pub suite_id: String,
    /// The level id the import binds (`snapshot_out`).
    pub unbound_level_id: String,
    /// The pre-registered spec the conformance run executes.
    pub spec: crate::experiment::ExperimentSpec,
    /// The retention set ref (the must-not-regress slice).
    pub retention_set_ref: String,
    /// The per-rule compliance metrics (`rule_id → metric`).
    pub compliance_rules: BTreeMap<String, String>,
    /// The margins the pre-registration declares (`metric → margin`).
    pub margins: BTreeMap<String, Json>,
}

impl RegressionSuiteExport {
    /// The canonical JSON — the suite rides the spec body plus the
    /// export-only members (maturity/preview stamped).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("suite_id".into(), Json::str(&self.suite_id));
        m.insert("harness_regression_suite".into(), Json::str("1"));
        m.insert("unbound_level_id".into(), Json::str(&self.unbound_level_id));
        m.insert("spec".into(), self.spec.to_json());
        m.insert(
            "retention_set_ref".into(),
            Json::str(&self.retention_set_ref),
        );
        m.insert(
            "compliance_rules".into(),
            Json::Obj(
                self.compliance_rules
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(v)))
                    .collect(),
            ),
        );
        m.insert("margins".into(), Json::Obj(self.margins.clone()));
        m.insert("maturity".into(), Json::str(MATURITY));
        m.insert("label".into(), Json::str(PREVIEW_LABEL));
        Json::Obj(m)
    }
}

/// The `snapshot_out` level's unbound ref — the level the import fills
/// (`ref_` stays a declared sentinel, never a guess).
pub const SNAPSHOT_OUT_LEVEL: &str = "snapshot_out";
/// The unbound ref sentinel — the import replaces it with the imported
/// `snapshot_id`.
pub const UNBOUND_REF: &str = "unbound:snapshot_out";

/// `export_regression_suite(definition_ref, snapshot_in, pins, …)` —
/// build the pre-registered paired comparison the post-training
/// conformance run executes (§5h.8 §2.1; the suite is an ordinary
/// `ExperimentSpec` — no new design kind).
///
/// `pins` supplies the records the spec binds (suite ref, split
/// assignment, eval/search budget refs, frozen artifact refs,
/// environment/model level refs) — records-in, never resolved here.
pub fn export_regression_suite(
    definition_ref: &str,
    snapshot_in: &str,
    pins: &RegressionSuitePins,
    retention_set_ref: &str,
    compliance_rules: BTreeMap<String, String>,
    margins: Json,
) -> Result<RegressionSuiteExport, CoEvolutionError> {
    use crate::experiment::{
        ArmSpec, BundlePolicy, ExperimentKind, ExperimentSpec, FactorSpec, LevelSpec, SuiteBinding,
        ValidationStrategy,
    };
    use hh_ontology::eval::{
        Design, DesignKind, FactorDeclaration, FactorLevel, ModelRole, Pairing, PreRegistration,
        RoutingPolicy,
    };
    use hh_ontology::FactorKind;

    let suite_id = format!("lab/regression/{definition_ref}@{snapshot_in}");
    let primary_metrics: Vec<String> = compliance_rules.values().cloned().collect();
    let prereg = PreRegistration {
        registered_at: pins.registered_at,
        hypothesis: format!(
            "snapshot_out ≽ snapshot_in under {definition_ref} on the per-rule compliance metrics \
             (margins per declaration) — the co-evolution regression suite (preview)"
        ),
        primary_metrics,
        // The per-metric margins ride `equivalence_margin` — the
        // pre-registration's typed margin member (the suite's verdict is
        // a non-inferiority claim).
        equivalence_margin: Some(margins.clone()),
        min_n: pins.replicates_per_cell,
        analysis_plan_ref: pins.analysis_plan_ref.clone(),
        task_split_hash: pins.task_split_hash.clone(),
        interactions: vec![],
    };
    let match_spec = hh_budget::matchspec::MatchSpec {
        // The snapshot pair crosses models by construction — the
        // matched-cap mode + `cross_model` scope name the comparison's
        // shape; `cold_start` holds the cache factor fixed.
        dimensions: vec![
            hh_ontology::dimensions::DimensionId::TokensInputUncached,
            hh_ontology::dimensions::DimensionId::TokensInputCacheRead,
            hh_ontology::dimensions::DimensionId::TokensInputCacheWrite,
            hh_ontology::dimensions::DimensionId::TokensOutputVisible,
            hh_ontology::dimensions::DimensionId::TokensOutputReasoning,
            hh_ontology::dimensions::DimensionId::ModelCalls,
            hh_ontology::dimensions::DimensionId::TimeWallMs,
        ],
        mode: hh_budget::MatchMode::MatchedCap,
        tolerance_ppm: 100_000,
        pricing_table_ref: pins.pricing_table_ref.clone(),
        model_scope: hh_budget::matchspec::ModelScope::CrossModel,
        cache_policy: hh_budget::matchspec::CachePolicy::ColdStart,
        utilization_floor_ppm: None,
    };
    let level = |id: &str, r: &str, label: &str| LevelSpec {
        level_id: id.to_string(),
        ref_: r.to_string(),
        overrides: None,
        label: label.to_string(),
        class: hh_ontology::participant::ParticipantClass::Native,
        non_portable: false,
    };
    let decl_level = |id: &str, r: &str, label: &str| FactorLevel {
        id: id.to_string(),
        content_ref: r.to_string(),
        label: label.to_string(),
    };
    let artifact = hh_ontology::config::Ref::new(
        pins.artifact_semantic_id.clone(),
        pins.artifact_version_id.clone(),
    );
    let arm = |arm_id: &str, hyp: &str, lvl: &str, artifact: &hh_ontology::config::Ref| ArmSpec {
        arm_id: arm_id.to_string(),
        hypothesis: hyp.to_string(),
        level_assignment: BTreeMap::from([
            ("model_snapshot".to_string(), lvl.to_string()),
            ("environment".to_string(), pins.environment_level_id.clone()),
        ]),
        eval_budget: pins.eval_budget_ref.clone(),
        search_budget: Some(pins.search_budget_ref.clone()),
        inference_budget: None,
        match_spec: Some(match_spec.clone()),
        artifact_ref: artifact.clone(),
        limits_enforced: "full".to_string(),
        model_role_table_ref: None,
        response_cache: None,
        ensemble_k: None,
    };
    let mut spec = ExperimentSpec {
        experiment_id: String::new(),
        kind: ExperimentKind::Comparative,
        design: Design {
            id: suite_id.clone(),
            kind: DesignKind::Paired,
            factors: vec![
                FactorDeclaration {
                    name: "model_snapshot".to_string(),
                    kind: FactorKind::ModelSnapshot,
                    granularity: None,
                    levels: vec![
                        decl_level("snapshot_in", snapshot_in, "the pre-training snapshot"),
                        decl_level(
                            SNAPSHOT_OUT_LEVEL,
                            UNBOUND_REF,
                            "the post-training snapshot (bound at import)",
                        ),
                    ],
                    role: Some(ModelRole::Primary),
                },
                FactorDeclaration {
                    name: "environment".to_string(),
                    kind: FactorKind::Environment,
                    granularity: None,
                    levels: vec![decl_level(
                        &pins.environment_level_id,
                        &pins.environment_level_ref,
                        "the pinned environment",
                    )],
                    role: None,
                },
            ],
            blocking: vec!["task".to_string()],
            replicates_per_cell: pins.replicates_per_cell,
            pairing: Pairing::ByTask,
            seed_policy: pins.seed_policy.clone(),
            held_out_split_ref: None,
            pre_registration: prereg.clone(),
            registry_snapshot_id: Some(pins.registry_snapshot_id.clone()),
            generators: None,
            resolution: None,
            routing_policy: RoutingPolicy::FailFast,
            deviation_policy: None,
            cache_na_stratified: false,
        },
        pre_registration: Some(prereg),
        factors: vec![
            FactorSpec {
                name: "model_snapshot".to_string(),
                kind: FactorKind::ModelSnapshot,
                granularity: None,
                role: Some("primary".to_string()),
                levels: vec![
                    level("snapshot_in", snapshot_in, "the pre-training snapshot"),
                    level(
                        SNAPSHOT_OUT_LEVEL,
                        UNBOUND_REF,
                        "the post-training snapshot (bound at import)",
                    ),
                ],
            },
            FactorSpec {
                name: "environment".to_string(),
                kind: FactorKind::Environment,
                granularity: None,
                role: Some("blocking".to_string()),
                levels: vec![level(
                    &pins.environment_level_id,
                    &pins.environment_level_ref,
                    "the pinned environment",
                )],
            },
        ],
        arms: vec![
            arm(
                "arm:snapshot_in",
                "pre-training snapshot baseline",
                "snapshot_in",
                &artifact,
            ),
            arm(
                "arm:snapshot_out",
                "post-training snapshot is non-inferior on compliance",
                SNAPSHOT_OUT_LEVEL,
                &artifact,
            ),
        ],
        suite: SuiteBinding {
            suite_ref: pins.suite_ref.clone(),
            split_labels_used: pins.split_labels_used.clone(),
            split_assignment_ref: Some(pins.split_assignment_ref.clone()),
        },
        replicates_per_cell: pins.replicates_per_cell,
        seed_policy: pins.seed_policy.clone(),
        validation_strategy: ValidationStrategy::FullSet,
        scheduling: pins.scheduling.clone(),
        reattempt: pins.reattempt.clone(),
        budgets: pins.budgets.clone(),
        bundle_policy: BundlePolicy::Named {
            name: suite_id.clone(),
        },
        ext: BTreeMap::from([
            (
                "unbound_level_id".to_string(),
                Json::str(SNAPSHOT_OUT_LEVEL),
            ),
            (
                "compliance_rules".to_string(),
                Json::Obj(
                    compliance_rules
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::str(v)))
                        .collect(),
                ),
            ),
        ]),
    };
    spec.experiment_id = spec.experiment_id();
    Ok(RegressionSuiteExport {
        suite_id,
        unbound_level_id: SNAPSHOT_OUT_LEVEL.to_string(),
        spec,
        retention_set_ref: retention_set_ref.to_string(),
        compliance_rules,
        margins: match margins {
            Json::Obj(m) => m,
            _ => BTreeMap::new(),
        },
    })
}

/// The records `export_regression_suite` binds (records-in — the caller
/// resolves; the export never reaches the registry itself).
#[derive(Debug, Clone, PartialEq)]
pub struct RegressionSuitePins {
    /// The suite ref the `SuiteBinding` names.
    pub suite_ref: String,
    /// The split labels the suite may use.
    pub split_labels_used: Vec<hh_ontology::lab::SplitLabel>,
    /// The `SplitAssignmentRecord` ref.
    pub split_assignment_ref: String,
    /// The pinned `eval_budget` ref.
    pub eval_budget_ref: String,
    /// The pinned `search_budget` ref.
    pub search_budget_ref: String,
    /// The frozen artifact's semantic id.
    pub artifact_semantic_id: String,
    /// The frozen artifact's version id (sealed — `<algo>:<hex>`).
    pub artifact_version_id: String,
    /// The environment level id + content ref.
    pub environment_level_id: String,
    /// The environment level's content ref.
    pub environment_level_ref: String,
    /// The registry snapshot the pre-registration seals under.
    pub registry_snapshot_id: String,
    /// The pinned analysis-plan ref.
    pub analysis_plan_ref: String,
    /// The `sha256:` task-split hash (predates the search — ADR-0143 L3).
    pub task_split_hash: String,
    /// The cross-model pricing table ref (`cross_model` MatchSpec
    /// requires one where spend matches — `None` names a
    /// token-only match).
    pub pricing_table_ref: Option<hh_budget::pricing::PricingTableRef>,
    /// The transaction seq the pre-registration commits at.
    pub registered_at: u64,
    /// Replicates per cell.
    pub replicates_per_cell: u32,
    /// The seed policy.
    pub seed_policy: hh_ontology::eval::SeedPolicy,
    /// The scheduling declaration.
    pub scheduling: crate::experiment::SchedulingPolicy,
    /// The reattempt policy.
    pub reattempt: crate::experiment::ReattemptPolicy,
    /// The budgets declaration.
    pub budgets: crate::experiment::ExperimentBudgets,
}

// ── export_compatibility_tags — the CompatibilityTagSet/1 projection ──────

/// `CompatibilityTagSet{snapshot_id, verified[], drifted[], broken[],
/// unknown[], rules_tagged{rule → tag}}` — the read-only projection over
/// the debt records + `CompatibilityRecord`s (§5h.8 §2.1). The tag set
/// is a *view* — it never mutates the underlying debt records.
#[derive(Debug, Clone, PartialEq)]
pub struct CompatibilityTagSet {
    /// The snapshot the tags name.
    pub snapshot_id: String,
    /// `verified` pairs (`definition@profile`).
    pub verified: Vec<String>,
    /// `drifted` pairs.
    pub drifted: Vec<String>,
    /// `broken` pairs.
    pub broken: Vec<String>,
    /// `unknown` pairs.
    pub unknown: Vec<String>,
    /// `rule_id → tag` — the per-rule projection (`trained_under`,
    /// `verified`, `drifted`, `broken`, `unknown`, `expiring`).
    pub rules_tagged: BTreeMap<String, String>,
}

impl CompatibilityTagSet {
    /// The canonical JSON (maturity/preview stamped — a projection, not
    /// an authority).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("compatibility_tag_set".into(), Json::str("1"));
        m.insert("snapshot_id".into(), Json::str(&self.snapshot_id));
        for (k, v) in [
            ("verified", &self.verified),
            ("drifted", &self.drifted),
            ("broken", &self.broken),
            ("unknown", &self.unknown),
        ] {
            m.insert(k.into(), Json::Arr(v.iter().map(Json::str).collect()));
        }
        m.insert(
            "rules_tagged".into(),
            Json::Obj(
                self.rules_tagged
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::str(v)))
                    .collect(),
            ),
        );
        m.insert("maturity".into(), Json::str(MATURITY));
        m.insert("label".into(), Json::str(PREVIEW_LABEL));
        Json::Obj(m)
    }
}

/// `export_compatibility_tags(snapshot_id, records, debts)` — the
/// read-only fold: every `CompatibilityRecord` naming `snapshot_id`
/// lands its `definition@profile` pair under the status bucket, and
/// every `model_conditioned`/`cross_model` debt record tags its rule by
/// the record's status (or `expiring` when the record's
/// `model_version_change` expiry fired).
pub fn export_compatibility_tags(
    snapshot_id: &str,
    records: &[CompatibilityRecord],
    debts: &[hh_hir::records::AssumptionDebtRecord],
) -> CompatibilityTagSet {
    let mut set = CompatibilityTagSet {
        snapshot_id: snapshot_id.to_string(),
        verified: Vec::new(),
        drifted: Vec::new(),
        broken: Vec::new(),
        unknown: Vec::new(),
        rules_tagged: BTreeMap::new(),
    };
    for r in records {
        if r.snapshot_ref != snapshot_id {
            continue;
        }
        let pair = format!("{}@{}", r.definition_semantic_id, r.profile_semantic_id);
        match &r.status {
            CompatibilityStatus::Verified | CompatibilityStatus::TrainedUnder => {
                set.verified.push(pair)
            }
            CompatibilityStatus::Drifted { rules } => {
                set.drifted.push(pair);
                for rule in rules {
                    set.rules_tagged.insert(rule.clone(), "drifted".to_string());
                }
            }
            CompatibilityStatus::Broken { .. } => set.broken.push(pair),
            CompatibilityStatus::Unknown => set.unknown.push(pair),
        }
    }
    for d in debts {
        use hh_ontology::debt::{DebtStatus, ExpiryKind};
        let tag = match d.status {
            DebtStatus::Expiring | DebtStatus::Expired => Some("expiring"),
            DebtStatus::Retired => Some("retired"),
            DebtStatus::Active => {
                // An active model-conditioned debt tags `conditioned`;
                // a rule the snapshot's records name `drifted` keeps
                // that tag (the loop above already wrote it).
                if set.rules_tagged.contains_key(&d.rule_id) {
                    None
                } else {
                    d.expiry
                        .as_ref()
                        .filter(|x| x.condition == ExpiryKind::ModelVersionChange)
                        .map(|_| "conditioned")
                }
            }
        };
        if let Some(t) = tag {
            set.rules_tagged.insert(d.rule_id.clone(), t.to_string());
        }
    }
    set
}

// ── import_snapshot — the claims-only import (I-1..I-4) ───────────────────

/// The sealed `(definition, profile)` pairs the import scores
/// compatibility for (records-in — the registry view the caller holds).
#[derive(Debug, Clone, PartialEq)]
pub struct SealedDefinition {
    /// The definition semantic id.
    pub definition_semantic_id: String,
    /// The profile semantic id.
    pub profile_semantic_id: String,
}

/// How the caller resolved the claim's `serving_route` (I-4a — the
/// import never probes the route itself; the caller's check is
/// records-in).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteStatus {
    /// The route resolves.
    Reachable,
    /// The route fails — `ServingRouteUnreachable`.
    Unreachable,
    /// No route check ran (an unchecked route is admissible — the claim
    /// records what the caller verified, never what it guessed).
    Unchecked,
}

/// `ImportOutcome{snapshot_record, compatibility[], scoped_rules[],
/// lifecycle}` — the import's records-out (I-1..I-4):
/// - `snapshot_record` — the `ModelSnapshotRecord` JSON
///   (`record_kind: model_snapshot`, `reported`/`unpinned` — the claim
///   never pins identity).
/// - `compatibility[]` — `trained_under` for every
///   `trained_under`-named `(definition, profile)`; `unknown` for every
///   other sealed definition (I-3).
/// - `scoped_rules[]` — the debt `rule_id`s whose
///   `scope.model_selectors` cover the imported snapshot — the
///   `model_version_change` sweep inputs (I-4).
/// - `lifecycle` — the `lifecycle.registry.imported` payload the caller
///   mints (the import authors the row; it never appends — records-out).
#[derive(Debug, Clone, PartialEq)]
pub struct ImportOutcome {
    /// The `ModelSnapshotRecord` JSON.
    pub snapshot_record: Json,
    /// The compatibility records (I-3's `trained_under`/`unknown` set).
    pub compatibility: Vec<CompatibilityRecord>,
    /// The debts the `model_version_change` sweep covers.
    pub scoped_rules: Vec<String>,
    /// The `lifecycle.registry.imported` payload.
    pub lifecycle: Json,
}

/// Whether a `ModelSelector` covers the imported claim
/// (`exact{model_id}` matches `model_id`; `range{family,
/// version_predicate}` matches `family == model_id` with the predicate
/// satisfied by the snapshot id — `*`/`>=`/`<=` prefixes are prefix-
/// matched, `*` = all).
pub fn selector_covers_claim(sel: &ModelSelector, claim: &SnapshotClaim) -> bool {
    match sel {
        ModelSelector::Exact { model_id } => model_id == &claim.model_id,
        ModelSelector::Range {
            family,
            version_predicate,
        } => {
            if family != &claim.model_id && family != &claim.provider {
                return false;
            }
            let p = version_predicate.trim();
            if p == "*" || p.is_empty() {
                return true;
            }
            if let Some(rest) = p.strip_prefix(">=") {
                return claim.snapshot_id.as_str() >= rest.trim();
            }
            if let Some(rest) = p.strip_prefix("<=") {
                return claim.snapshot_id.as_str() <= rest.trim();
            }
            if let Some(prefix) = p.strip_suffix('*') {
                return claim.snapshot_id.starts_with(prefix);
            }
            claim.snapshot_id == p
        }
    }
}

/// `import_snapshot(claim, sealed_definitions, debts, route_status,
/// known_snapshot_ids, descendant_snapshot_ids, provenance, created_at)` — the
/// claims-only import (I-1..I-4; §5h.8 §2.2).
///
/// - **I-0/I-1** — the claim validates member-level (`provenance`
///   present, `snapshot_id`/`provider`/`model_id` non-empty —
///   `ClaimSchemaInvalid`); `policy_version_exposed ∉ {supported}` leaves
///   every compatibility `unknown` (the pin is the guard's precondition,
///   not the import's refusal — the claim lands honestly *and* the guard
///   refuses bind).
/// - **I-2** — `ModelSnapshotRecord` is `reported`/`unpinned`: the claim
///   is data, never a `ContentAddress` (N8's foreign-digest rule).
/// - **I-3** — `trained_under` records for the claim's named
///   `(definition, profile)` pairs; `unknown` for every other sealed
///   definition.
/// - **I-4** — `scoped_rules[]` collects every debt whose
///   `scope.model_selectors` covers the claim — the caller mints
///   `model_version_change` transitions + schedules the reverse sweep.
/// - **refusals** — `BaseSnapshotUnknown` (the claim's `base_snapshot_ref`
///   resolves no known snapshot), `ServingRouteUnreachable` (the caller
///   reports the route dead), `LineageCycle` (the claim's own snapshot id
///   already descends from a `trained_under` lineage member).
#[allow(clippy::too_many_arguments)] // the import's records-in arity is the §2.2 record shape.
pub fn import_snapshot(
    claim: &SnapshotClaim,
    sealed_definitions: &[SealedDefinition],
    debts: &[hh_hir::records::AssumptionDebtRecord],
    route_status: RouteStatus,
    known_snapshot_ids: &[String],
    descendant_snapshot_ids: &[String],
    provenance: &ProvenanceRecord,
    created_at: u64,
) -> Result<ImportOutcome, CoEvolutionError> {
    // I-0 — member-level claim validation (the claim carries no
    // `provenance` member — claims inside records carry it there; the
    // caller supplies the import's provenance as records-in).
    if claim.snapshot_id.is_empty() || claim.provider.is_empty() || claim.model_id.is_empty() {
        return Err(CoEvolutionError::ClaimSchemaInvalid {
            detail: "provider/model_id/snapshot_id are mandatory".into(),
        });
    }
    if let Some(base) = &claim.base_snapshot_ref {
        if !known_snapshot_ids.iter().any(|k| k == base) {
            return Err(CoEvolutionError::BaseSnapshotUnknown {
                base_ref: base.clone(),
            });
        }
    }
    if matches!(route_status, RouteStatus::Unreachable) {
        return Err(CoEvolutionError::ServingRouteUnreachable {
            route: claim.serving_route.clone().unwrap_or_default(),
        });
    }
    if descendant_snapshot_ids
        .iter()
        .any(|d| d == &claim.snapshot_id)
    {
        return Err(CoEvolutionError::LineageCycle {
            snapshot_id: claim.snapshot_id.clone(),
        });
    }
    // I-3 — the trained_under/unknown compatibility set.
    let pinned = matches!(
        claim.policy_version_exposed,
        PolicyVersionExposed::Supported
    );
    let mut compatibility = Vec::new();
    let mut trained: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for t in &claim.trained_under {
        trained
            .entry(t.definition_semantic_id.clone())
            .or_default()
            .push(t.profile_semantic_id.clone());
    }
    for d in sealed_definitions {
        let named = trained
            .get(&d.definition_semantic_id)
            .map(|ps| ps.iter().any(|p| p == &d.profile_semantic_id))
            .unwrap_or(false);
        let status = if !pinned {
            CompatibilityStatus::Unknown
        } else if named {
            CompatibilityStatus::TrainedUnder
        } else {
            CompatibilityStatus::Unknown
        };
        let evidence = match &status {
            CompatibilityStatus::TrainedUnder => Some(format!("claim:{}", claim.snapshot_id)),
            _ => None,
        };
        compatibility.push(CompatibilityRecord {
            snapshot_ref: claim.snapshot_id.clone(),
            definition_semantic_id: d.definition_semantic_id.clone(),
            profile_semantic_id: d.profile_semantic_id.clone(),
            status,
            evidence_ref: evidence,
            regression_suite_ref: None,
            created_at,
            expiry_condition: None,
            provenance: provenance.clone(),
        });
    }
    // I-4 — the model_version_change sweep inputs.
    let scoped_rules: Vec<String> = debts
        .iter()
        .filter(|d| {
            d.scope
                .as_ref()
                .map(|s| {
                    s.model_selectors
                        .iter()
                        .any(|sel| selector_covers_claim(sel, claim))
                })
                .unwrap_or(false)
        })
        .map(|d| d.rule_id.clone())
        .collect();
    // The ModelSnapshotRecord — `reported`/`unpinned`, `imported: true`.
    let mut snapshot = BTreeMap::new();
    snapshot.insert("record_kind".into(), Json::str("model_snapshot"));
    snapshot.insert(
        "semantic_id".into(),
        Json::str(format!("{}/{}", claim.provider, claim.model_id)),
    );
    snapshot.insert("snapshot_id".into(), Json::str(&claim.snapshot_id));
    snapshot.insert("provider".into(), Json::str(&claim.provider));
    snapshot.insert("model_id".into(), Json::str(&claim.model_id));
    snapshot.insert("status".into(), Json::str("reported"));
    snapshot.insert("pinned".into(), Json::Bool(false));
    snapshot.insert("imported".into(), Json::Bool(true));
    if let Some(r) = &claim.serving_route {
        snapshot.insert("serving_route".into(), Json::str(r));
    }
    if let Some(b) = &claim.base_snapshot_ref {
        snapshot.insert("base_snapshot_ref".into(), Json::str(b));
    }
    if let Some(w) = &claim.weights_digest {
        snapshot.insert("weights_digest".into(), w.to_json());
    }
    snapshot.insert(
        "policy_version_exposed".into(),
        Json::str(claim.policy_version_exposed.name()),
    );
    if let Some(t) = &claim.training_cutoff_claim {
        snapshot.insert("training_cutoff_claim".into(), t.to_json());
    }
    if let Some(l) = &claim.training_lineage {
        snapshot.insert("training_lineage".into(), l.to_json());
    }
    snapshot.insert(
        "trained_under".into(),
        Json::Arr(claim.trained_under.iter().map(|t| t.to_json()).collect()),
    );
    snapshot.insert("maturity".into(), Json::str(MATURITY));
    let snapshot_record = Json::Obj(snapshot);
    let lifecycle = Json::obj([
        ("class", Json::str("lifecycle.registry.imported")),
        ("subject_kind", Json::str("model_snapshot")),
        ("snapshot_id", Json::str(&claim.snapshot_id)),
        (
            "semantic_id",
            Json::str(format!("{}/{}", claim.provider, claim.model_id)),
        ),
        (
            "trained_under_count",
            Json::Int(claim.trained_under.len() as i64),
        ),
        (
            "scoped_rules",
            Json::Arr(scoped_rules.iter().map(Json::str).collect()),
        ),
        ("maturity", Json::str(MATURITY)),
    ]);
    Ok(ImportOutcome {
        snapshot_record,
        compatibility,
        scoped_rules,
        lifecycle,
    })
}

// ── guard_at_bind — the full compatibility-aware bind guard (G-1..G-3) ────

/// The arm's declared origin (G-3's evolution-origin leg).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArmOrigin {
    /// A registered/hand-authored arm.
    Registered,
    /// An evolution-origin arm (the campaign produced it).
    Evolution,
}

/// The bind guard's verdict (§5h.8 §3):
/// - `proceed` — `verified` compatibility.
/// - `annotated{compatibility, expiring_rules[]}` — `unknown`/`drifted`
///   proceed with the annotation attached to the arm's report (drifted
///   names its rules).
/// - `refused{code}` — `broken` (`CompatibilityBroken`) or an
///   evolution-origin arm with no record (`ExploratoryOnly`, lifted by
///   the `exploratory` experiment kind or `allow_unverified_snapshot`).
#[derive(Debug, Clone, PartialEq)]
pub enum BindGuard {
    /// Bind proceeds clean.
    Proceed,
    /// Bind proceeds with the annotation.
    Annotated {
        /// The compatibility status spelling (`unknown`/`drifted`).
        compatibility: String,
        /// The `drifted` rules that annotate the arm.
        expiring_rules: Vec<String>,
    },
    /// Bind refuses.
    Refused {
        /// The closed refusal code.
        code: CoEvolutionError,
    },
}

/// `guard_at_bind(compatibility, arm_origin, exploratory,
/// allow_unverified_snapshot)` — the full G-1..G-3 guard over the
/// import's `CompatibilityRecord` (the v0 `guard_at_bind` in `model.rs`
/// is the absence-of-claim half; this is the evidence-backed verdict).
///
/// - `record = None` — an evolution-origin arm refuses `ExploratoryOnly`
///   unless the experiment declared `exploratory` or the caller passed
///   `allow_unverified_snapshot`; a registered arm proceeds `annotated`
///   with `compatibility = unknown` (an unpinned snapshot is a claim,
///   never evidence).
/// - `verified`/`trained_under` → `proceed`.
/// - `drifted`/`unknown` → `annotated` (the drifted rules ride the
///   annotation).
/// - `broken` → `refused{CompatibilityBroken{report_ref}}`.
pub fn guard_at_bind(
    record: Option<&CompatibilityRecord>,
    arm_origin: ArmOrigin,
    exploratory: bool,
    allow_unverified_snapshot: bool,
) -> BindGuard {
    match record {
        None => {
            if matches!(arm_origin, ArmOrigin::Evolution)
                && !exploratory
                && !allow_unverified_snapshot
            {
                BindGuard::Refused {
                    code: CoEvolutionError::ExploratoryOnly {
                        detail: "evolution-origin arm binds a snapshot with no \
                                 CompatibilityRecord — declare exploratory or \
                                 allow_unverified_snapshot"
                            .into(),
                    },
                }
            } else {
                BindGuard::Annotated {
                    compatibility: "unknown".to_string(),
                    expiring_rules: Vec::new(),
                }
            }
        }
        Some(r) => match &r.status {
            CompatibilityStatus::Verified | CompatibilityStatus::TrainedUnder => BindGuard::Proceed,
            CompatibilityStatus::Drifted { rules } => BindGuard::Annotated {
                compatibility: "drifted".to_string(),
                expiring_rules: rules.clone(),
            },
            CompatibilityStatus::Unknown => BindGuard::Annotated {
                compatibility: "unknown".to_string(),
                expiring_rules: Vec::new(),
            },
            CompatibilityStatus::Broken { report_ref } => BindGuard::Refused {
                code: CoEvolutionError::CompatibilityBroken {
                    report_ref: report_ref.clone(),
                },
            },
        },
    }
}

// ── post_import_sweep — the model_version_change trigger family ──────────

/// `PostImportSweep{scheduled[], covered[], unchanged[]}` — the reverse
/// sweep's records-out (§5h.8 §2.3): for every definition the Lab binds
/// to the imported snapshot, the scoped rules whose
/// `scope.model_selectors` do **not** cover it are scheduled for removal
/// tests (`lifecycle.debt.removal_test.scheduled` — the manager mints);
/// rules that cover it carry their `model_version_change` trigger
/// (already collected by [`import_snapshot`]'s `scoped_rules`).
#[derive(Debug, Clone, PartialEq)]
pub struct PostImportSweep {
    /// The `rule_id`s to schedule removal tests for (scope ∌ snapshot_out).
    pub scheduled: Vec<String>,
    /// The `rule_id`s the snapshot covers (the `model_version_change`
    /// side — `expiry_condition` fires the trigger).
    pub covered: Vec<String>,
    /// Scoped rules with no `model_selectors` at all (unscoped records
    /// never schedule — the sweep is scope-conditioned).
    pub unchanged: Vec<String>,
}

/// `post_import_sweep(debts, claim)` — the records-out fold: a debt
/// carrying `scope.model_selectors` is either *covered* (any selector
/// matches the imported snapshot — the `model_version_change` trigger
/// side) or *scheduled* (none match — the reverse-sweep side);
/// unscoped debts are `unchanged`.
pub fn post_import_sweep(
    debts: &[hh_hir::records::AssumptionDebtRecord],
    claim: &SnapshotClaim,
) -> PostImportSweep {
    let mut out = PostImportSweep {
        scheduled: Vec::new(),
        covered: Vec::new(),
        unchanged: Vec::new(),
    };
    for d in debts {
        let scope = match &d.scope {
            Some(s) if !s.model_selectors.is_empty() => s,
            _ => {
                out.unchanged.push(d.rule_id.clone());
                continue;
            }
        };
        if scope
            .model_selectors
            .iter()
            .any(|sel| selector_covers_claim(sel, claim))
        {
            out.covered.push(d.rule_id.clone());
        } else {
            out.scheduled.push(d.rule_id.clone());
        }
    }
    out
}

// ── The interface's own debt record (AC-R-2.9.8-13) ──────────────────────

/// The stable `rule_id` the interface's reflexive debt record carries.
pub const INTERFACE_DEBT_RULE_ID: &str = "rule:coevolution-interface";

/// The conformance-suite ref the interface record's removal test names —
/// the null-trainer round-trip ([`NullTrainer`]) is the executable
/// discharge path.
pub const NULL_TRAINER_SUITE_REF: &str = "coevolution:null-trainer-round-trip";

/// `interface_debt_record(model_family, owner, created_by, created_at)` —
/// the interface's **own** `AssumptionDebtRecord` (AC-R-2.9.8-13; the
/// same discipline the spec-debt register applies to ADR-0202/0203/0204):
///
/// - `debt_class: model_conditioned` — the interface's value claim holds
///   only for a trainable beneficiary of the named model generation;
///   `scope.model_selectors` covers `range{family, generation}` so
///   [`post_import_sweep`] treats it as covered on import and the
///   `model_version_change` expiry fires on a generation change (the
///   spec-debt register's T5 trigger, landed as a record);
/// - `hypothesis_typed` — `premature_stop`/`increase`: the trainable-
///   beneficiary assumption is a hypothesis until `matched_total`
///   evidence grades it;
/// - `removal_test{kind: conformance_run, conformance_suite_ref:
///   NULL_TRAINER_SUITE_REF}` — the null-trainer round-trip the ticket's
///   fixture executes (export → `NullTrainer::train` → `import_snapshot`
///   → `post_import_sweep`), never an unverifiable promise;
/// - `revalidation{on: [model_change, schedule], action:
///   re_probe}` — the revalidation cadence every C4 mechanism owes.
pub fn interface_debt_record(
    model_family: &str,
    owner: &OwnerRef,
    created_by: &ProvenanceRecord,
    created_at: u64,
) -> hh_hir::records::AssumptionDebtRecord {
    use hh_hir::leaves::Text;
    hh_hir::records::AssumptionDebtRecord {
        rule_id: INTERFACE_DEBT_RULE_ID.to_string(),
        hypothesis: Text::new(
            String::from(
                "the co-evolution interface's value claim holds only for a \
                 trainable beneficiary of the named model generation and only \
                 under matched_total evidence — a generation change expires \
                 the claim and re-runs the null-trainer round-trip (AC-R-2.9.8-13; \
                 the spec-debt register's ADR-0202/0203/0204 rows landed as a \
                 record)",
            ),
            owner.id.as_str(),
            created_by.clone(),
        ),
        evidence_refs: vec![EvidenceRef {
            kind: EvidenceKind::Source,
            reference: NULL_TRAINER_SUITE_REF.to_string(),
            observed_at: Some(created_at),
            tier: None,
            provisional: true,
        }],
        owner: owner.clone(),
        expiry_condition: ExpiryCondition {
            kind: ExpiryKind::ModelVersionChange,
            value: Some(model_family.to_string()),
        },
        removal_test_ref: NULL_TRAINER_SUITE_REF.to_string(),
        status: DebtStatus::Active,
        debt_class: Some(DebtClass::ModelConditioned),
        hypothesis_typed: Some(HypothesisTyped {
            subject: HypothesisSubject::Deficiency,
            deficiency_class: DeficiencyClass::PrematureStop,
            predicted_effect: PredictedEffect::Increase,
            metric_ref: Some("metric:artifact_benefit".into()),
        }),
        scope: Some(DebtScope {
            model_selectors: vec![ModelSelector::Range {
                family: model_family.to_string(),
                version_predicate: "*".to_string(),
            }],
            task_classes: vec![],
            roles: vec![],
        }),
        expiry: Some(DebtExpiry {
            condition: ExpiryKind::ModelVersionChange,
            params: ExpiryParams::default(),
        }),
        runway_ms: None,
        revalidation: Some(Revalidation {
            on: vec![RevalidationOn::ModelChange, RevalidationOn::Schedule],
            action: Some(RevalidationAction::ReProbe),
        }),
        removal_test: Some(RemovalTest {
            conformance_suite_ref: Some(NULL_TRAINER_SUITE_REF.to_string()),
            ..RemovalTest::new(RemovalTestKind::ConformanceRun)
        }),
        created_by: Some(created_by.clone()),
        created_at: Some(created_at),
        supersedes: None,
    }
}

// ── Consolidation (R-2.9.5 6d; §5h.8 §5) ──────────────────────────────────

/// `LessonFact{lesson_id, rule_id, kind, effect_size?, veto?,
/// provenance}` — the campaign's *lesson* the consolidation reads
/// (records-in; a lesson is evidence the `active` candidate produced,
/// never a proposal).
#[derive(Debug, Clone, PartialEq)]
pub struct LessonFact {
    /// The lesson's id.
    pub lesson_id: String,
    /// The rule the lesson concerns.
    pub rule_id: String,
    /// The lesson kind (a [`CONSOLIDABLE_LESSON_KINDS`] member is
    /// consolidation-eligible; anything else is never-consolidate).
    pub kind: String,
    /// The matched-budget effect the lesson claims (the
    /// `ComparisonReport` spelling — `harmed`/`non_harmed`/…).
    pub effect: Option<String>,
    /// The lesson's provenance.
    pub provenance: Option<Json>,
}

/// The lesson kinds the consolidation set admits (§5h.8 §5.1 C1–C5):
/// `task_level_ablation`, `formatting_improvement`,
/// `stop_rule_refinement`, `judge_calibration`, `name_adjustment`.
pub const CONSOLIDABLE_LESSON_KINDS: &[&str] = &[
    "task_level_ablation",
    "formatting_improvement",
    "stop_rule_refinement",
    "judge_calibration",
    "name_adjustment",
];

/// `ConsolidationPolicy{k_cycles, economics_factor}` — the view's
/// threshold policy (§5h.8 §5.1).
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationPolicy {
    /// The candidate must be `active` for at least this many cycles.
    pub k_cycles: u64,
    /// The projected net improvement must exceed `economics_factor` ×
    /// the per-cycle cost (a positive float; the view is honest — the
    /// economics row is `n/a` when the inputs don't bound it).
    pub economics_factor: f64,
}

impl Default for ConsolidationPolicy {
    fn default() -> ConsolidationPolicy {
        ConsolidationPolicy {
            k_cycles: 1,
            economics_factor: 1.0,
        }
    }
}

/// One candidate the view scores (records-in — the folded
/// `CandidateRecord` + its lesson facts + its live-rule status).
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationInput {
    /// The rule the candidate introduced/conditions.
    pub rule_id: String,
    /// The folded state (`active` only consolidates — §5h.8 §5.1 C-gate).
    pub state: String,
    /// Cycles the candidate has been `active` (the campaign's own count).
    pub cycles_active: u64,
    /// The candidate's lesson facts.
    pub lessons: Vec<LessonFact>,
    /// The projected net improvement ÷ per-cycle cost ratio the
    /// caller's economics row reports (`None` = `n/a` — the candidate
    /// fails the economics leg honestly, never by a fabricated bound).
    pub economics_ratio: Option<f64>,
    /// Whether the rule is already consolidated/retired.
    pub terminal: bool,
}

/// `NeverConsolidate` reasons (§5h.8 §5.1 N1–N5) — the closed "not a
/// candidate" set, each spelled.
#[derive(Debug, Clone, PartialEq)]
pub enum NeverConsolidate {
    /// N1 — the lesson is a judge/instrument calibration gap (the rule's
    /// effect belongs to the instrument, not the harness).
    JudgeCalibration,
    /// N2 — the lesson's effect is task-level only (a task-suite
    /// property, not a harness rule).
    TaskOnlyEffect,
    /// N3 — the rule gates a security/safety surface (veto-trippable —
    /// the safety kernel's surface never consolidates into weights).
    SafetyGated,
    /// N4 — the rule is already `retired`/`reverted`/`rejected` (a
    /// terminal rule is not a candidate).
    Terminal,
    /// N5 — the rule's debt record is absent or incomplete (a
    /// conditioned rule consolidates only with its debt record —
    /// §5h.8 §5.1's debt-precondition).
    DebtIncomplete,
}

impl NeverConsolidate {
    /// The canonical spelling.
    pub fn name(&self) -> &'static str {
        match self {
            NeverConsolidate::JudgeCalibration => "judge_calibration",
            NeverConsolidate::TaskOnlyEffect => "task_only_effect",
            NeverConsolidate::SafetyGated => "safety_gated",
            NeverConsolidate::Terminal => "terminal",
            NeverConsolidate::DebtIncomplete => "debt_incomplete",
        }
    }
}

/// `ConsolidationCandidate{rule_id, lessons[], rationale}` — the view's
/// output row (§5h.8 §5.1; a *view* — the candidate is a proposal
/// input, never an applied change).
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationCandidate {
    /// The rule the consolidation would absorb.
    pub rule_id: String,
    /// The lesson refs backing the proposal.
    pub lessons: Vec<String>,
    /// The rationale (the C-legs the candidate passed).
    pub rationale: String,
}

/// `consolidation_candidates(inputs, policy)` — the `consolidation_
/// candidates` view (R-2.9.5 6d): an `active` candidate ≥ `k_cycles`
/// whose lessons are consolidation-kind facts and whose economics row
/// bounds `economics_factor` lands a candidate row; the
/// never-consolidate legs (N1–N5) are evaluated first — a single
/// never-consolidate hit removes the row entirely.
pub fn consolidation_candidates(
    inputs: &[ConsolidationInput],
    debt_complete: &dyn Fn(&str) -> bool,
    policy: &ConsolidationPolicy,
) -> (Vec<ConsolidationCandidate>, Vec<(String, NeverConsolidate)>) {
    let mut candidates = Vec::new();
    let mut refused = Vec::new();
    for i in inputs {
        // N4 — a terminal rule is never a candidate.
        if i.terminal || i.state != "active" {
            refused.push((i.rule_id.clone(), NeverConsolidate::Terminal));
            continue;
        }
        // N1/N2 — lesson-kind legs: any calibration lesson removes the
        // row (the instrument's gap is not a harness rule); a candidate
        // with *no* consolidable lesson is task-only.
        if i.lessons.iter().any(|l| l.kind == "judge_calibration") {
            refused.push((i.rule_id.clone(), NeverConsolidate::JudgeCalibration));
            continue;
        }
        let consol_lessons: Vec<&LessonFact> = i
            .lessons
            .iter()
            .filter(|l| CONSOLIDABLE_LESSON_KINDS.contains(&l.kind.as_str()))
            .collect();
        if consol_lessons.is_empty() {
            refused.push((i.rule_id.clone(), NeverConsolidate::TaskOnlyEffect));
            continue;
        }
        // N3 — a safety-gated lesson veto removes the row.
        if i.lessons
            .iter()
            .any(|l| l.kind == "safety_gated" || l.kind == "veto")
        {
            refused.push((i.rule_id.clone(), NeverConsolidate::SafetyGated));
            continue;
        }
        // N5 — the debt precondition.
        if !debt_complete(&i.rule_id) {
            refused.push((i.rule_id.clone(), NeverConsolidate::DebtIncomplete));
            continue;
        }
        // C-gate — active ≥ k_cycles; the economics row bounds the
        // factor (an `n/a` economics row fails honestly).
        if i.cycles_active < policy.k_cycles {
            continue;
        }
        match i.economics_ratio {
            Some(r) if r > policy.economics_factor => {
                candidates.push(ConsolidationCandidate {
                    rule_id: i.rule_id.clone(),
                    lessons: consol_lessons.iter().map(|l| l.lesson_id.clone()).collect(),
                    rationale: format!(
                        "active ≥ {} cycles; consolidable lessons; economics {:.2} > {:.2}",
                        policy.k_cycles, r, policy.economics_factor
                    ),
                });
            }
            _ => continue,
        }
    }
    (candidates, refused)
}

/// `ConsolidationProposal{target_rule, lessons[], snapshot_in,
/// debt_refs[], provenance}` — the proposal (R-2.9.5: a proposal, never
/// an applied diff — the proposal rides to the human seal).
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationProposal {
    /// The rule the consolidation absorbs.
    pub target_rule: String,
    /// The lesson refs.
    pub lessons: Vec<String>,
    /// The snapshot the consolidation trains into.
    pub snapshot_in: String,
    /// The conditioned debt refs the proposal retires on `absorbed`.
    pub debt_refs: Vec<String>,
    /// The proposer's provenance.
    pub provenance: Json,
}

impl ConsolidationProposal {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("target_rule", Json::str(&self.target_rule)),
            (
                "lessons",
                Json::Arr(self.lessons.iter().map(Json::str).collect()),
            ),
            ("snapshot_in", Json::str(&self.snapshot_in)),
            (
                "debt_refs",
                Json::Arr(self.debt_refs.iter().map(Json::str).collect()),
            ),
            ("provenance", self.provenance.clone()),
            ("maturity", Json::str(MATURITY)),
            ("label", Json::str(PREVIEW_LABEL)),
        ])
    }
}

/// `ConsolidationEvidence` — one report's three-valued verdict the
/// consolidation reads (the report rows are records-in; the verdict
/// fold is closed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsolidationEvidence {
    /// The rule shows harm under the snapshot (absorption would carry
    /// the harm).
    Harmed,
    /// The rule shows no harm (the absorption's target criterion).
    NonHarmed,
    /// The report is inconclusive.
    Inconclusive,
}

/// `ConsolidationVerdict ∈ {absorbed, rejected, superseded}` — the
/// closed consolidation outcome (§5h.8 §5.1).
#[derive(Debug, Clone, PartialEq)]
pub enum ConsolidationVerdict {
    /// The rule absorbed into weights — the conditioned debts retire
    /// through the retirement experiment + human seal.
    Absorbed,
    /// The absorption is refused (harmed target, veto, or delivered-
    /// elsewhere).
    Rejected,
    /// The rule was already superseded (the report records the state,
    /// no retirement re-runs).
    Superseded,
}

impl ConsolidationVerdict {
    /// The canonical spelling.
    pub fn name(&self) -> &'static str {
        match self {
            ConsolidationVerdict::Absorbed => "absorbed",
            ConsolidationVerdict::Rejected => "rejected",
            ConsolidationVerdict::Superseded => "superseded",
        }
    }
}

/// `consolidation_verdict(target, retention, family, veto_tripped,
/// delivered_elsewhere, superseded)` — the verdict fold (§5h.8 §5.1
/// V1–V3):
///
/// - **V1** — the target's own `ComparisonReport` is mandatory
///   (`MissingComparisonReport`); `absorbed` never lands without it.
/// - **V2** — `veto_tripped`, `delivered_elsewhere`, a harmed target, a
///   regressed retention set, or a vetoed family ⇒ `rejected`.
/// - **V3** — an already-superseded rule reports `superseded` (before
///   every other leg — the retirement never re-runs).
/// - else ⇒ `absorbed`.
pub fn consolidation_verdict(
    target: Option<ConsolidationEvidence>,
    retention: ConsolidationEvidence,
    family_veto: bool,
    veto_tripped: bool,
    delivered_elsewhere: bool,
    superseded: bool,
) -> Result<ConsolidationVerdict, CoEvolutionError> {
    if superseded {
        return Ok(ConsolidationVerdict::Superseded);
    }
    let target = target.ok_or(CoEvolutionError::MissingComparisonReport {
        which: "target".into(),
    })?;
    if veto_tripped
        || delivered_elsewhere
        || family_veto
        || matches!(target, ConsolidationEvidence::Harmed)
        || matches!(retention, ConsolidationEvidence::Harmed)
    {
        return Ok(ConsolidationVerdict::Rejected);
    }
    Ok(ConsolidationVerdict::Absorbed)
}

/// `ConsolidationReport{report_id, kind, proposal?, target_rule,
/// snapshot_ref, lessons[], verdict, experiment_ref, evidence_refs[],
/// label, headline, maturity}` — the `AnalysisRecord`-shaped report the
/// retirement experiment names (§5h.8 §5.1; the `RetirementRecord`'s
/// `removal_test_report_ref` carries `report_id`).
#[derive(Debug, Clone, PartialEq)]
pub struct ConsolidationReport {
    /// The report's content id (`idp/1` over the body — `seal` fills).
    pub report_id: String,
    /// The proposal ref, where the report follows one.
    pub proposal_ref: Option<String>,
    /// The absorbed candidate's rule.
    pub target_rule: String,
    /// The post-training snapshot the consolidation ran under.
    pub snapshot_ref: String,
    /// The lesson refs the report cites.
    pub lessons: Vec<String>,
    /// The verdict.
    pub verdict: ConsolidationVerdict,
    /// The retirement experiment ref the verdict ran under.
    pub experiment_ref: String,
    /// The evidence refs (`ComparisonReport`/`ConsistencyReport`s).
    pub evidence_refs: Vec<String>,
    /// The conditioned debt refs the verdict retires.
    pub debt_refs: Vec<String>,
}

impl ConsolidationReport {
    /// The canonical JSON — `AnalysisRecord`-shaped (`kind: analysis`,
    /// `label: preview`, `headline: false` — the report never rides a
    /// headline result, §5h.8 §5.1's preview wall).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("kind".into(), Json::str("analysis"));
        m.insert("report_id".into(), Json::str(&self.report_id));
        m.insert("report_kind".into(), Json::str("consolidation_report"));
        if let Some(p) = &self.proposal_ref {
            m.insert("proposal_ref".into(), Json::str(p));
        }
        m.insert("target_rule".into(), Json::str(&self.target_rule));
        m.insert("snapshot_ref".into(), Json::str(&self.snapshot_ref));
        m.insert(
            "lessons".into(),
            Json::Arr(self.lessons.iter().map(Json::str).collect()),
        );
        m.insert("verdict".into(), Json::str(self.verdict.name()));
        m.insert("experiment_ref".into(), Json::str(&self.experiment_ref));
        m.insert(
            "evidence_refs".into(),
            Json::Arr(self.evidence_refs.iter().map(Json::str).collect()),
        );
        m.insert(
            "debt_refs".into(),
            Json::Arr(self.debt_refs.iter().map(Json::str).collect()),
        );
        m.insert("label".into(), Json::str(PREVIEW_LABEL));
        m.insert("headline".into(), Json::Bool(false));
        m.insert("maturity".into(), Json::str(MATURITY));
        Json::Obj(m)
    }

    /// Fill `report_id` with the body's `idp/1` address.
    pub fn seal(&mut self) {
        self.report_id = String::new();
        self.report_id = hh_identity::idp_id(
            "hh.consolidation_report",
            &self.to_json().to_canonical_string().into_bytes(),
        );
    }
}

/// `consolidation_retirement_record(report, verdict_ref, decided_by)`
/// — the `RetirementRecord` the human-sealed retirement carries
/// (§5h.8 §5.1): `rationale: "absorbed into weights — snapshot <ref>"`,
/// `removal_test_report_ref = report.report_id`, `verdict_ref` the
/// settle's `RemovalVerdict` — the debt manager's `retire` is the only
/// gate (R-2.9.6 D5/D7).
pub fn consolidation_retirement_record(
    report: &ConsolidationReport,
    verdict_ref: &str,
    decided_by: &ProvenanceRecord,
) -> hh_hir::debt::RetirementRecord {
    hh_hir::debt::RetirementRecord {
        removal_test_report_ref: report.report_id.clone(),
        verdict_ref: verdict_ref.to_string(),
        decided_by: decided_by.clone(),
        rationale: hh_hir::leaves::Text::new(
            format!(
                "absorbed into weights — snapshot {} (consolidation {} verdict {})",
                report.snapshot_ref,
                report.report_id,
                report.verdict.name()
            ),
            "consolidation",
            decided_by.clone(),
        ),
    }
}

// ── CoEvolutionCycleRecord / CyclePolicy — the sidecar records ────────────

/// `CyclePhase ∈ {harness_search, weight_update, re_evaluation,
/// consolidation}` — the closed phase sum (§5h.8 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CyclePhase {
    /// The harness-side evolution leg (an ordinary campaign over the
    /// cycle's `SearchBudgetRecord`).
    HarnessSearch,
    /// The external weight-update leg (export → train → import; the
    /// harness never trains — this phase's `outputs` cite the
    /// `training_run_ref` + imported snapshot).
    WeightUpdate,
    /// The regression-suite re-evaluation leg (the exported suite over
    /// `snapshot_out`).
    ReEvaluation,
    /// The consolidation leg (proposal → retirement experiment →
    /// human seal).
    Consolidation,
}

impl CyclePhase {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            CyclePhase::HarnessSearch => "harness_search",
            CyclePhase::WeightUpdate => "weight_update",
            CyclePhase::ReEvaluation => "re_evaluation",
            CyclePhase::Consolidation => "consolidation",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<CyclePhase> {
        match s {
            "harness_search" => Some(CyclePhase::HarnessSearch),
            "weight_update" => Some(CyclePhase::WeightUpdate),
            "re_evaluation" => Some(CyclePhase::ReEvaluation),
            "consolidation" => Some(CyclePhase::Consolidation),
            _ => None,
        }
    }

    /// The ordered phase cycle.
    pub fn order() -> [CyclePhase; 4] {
        [
            CyclePhase::HarnessSearch,
            CyclePhase::WeightUpdate,
            CyclePhase::ReEvaluation,
            CyclePhase::Consolidation,
        ]
    }
}

/// `SwitchRule ∈ {search_budget_exhausted, cycle_count, operator}` —
/// when the cycle switches phases (§5h.8 §6; the harness-leg half is a
/// `SearchBudgetRecord` bound; the weight-leg half is the declared
/// weight budget).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchRule {
    /// Switch when the harness-phase `SearchBudgetRecord` is exhausted.
    SearchBudgetExhausted,
    /// Switch on a fixed cycle count.
    CycleCount,
    /// Switch only on an explicit operator transition.
    Operator,
}

impl SwitchRule {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            SwitchRule::SearchBudgetExhausted => "search_budget_exhausted",
            SwitchRule::CycleCount => "cycle_count",
            SwitchRule::Operator => "operator",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<SwitchRule> {
        match s {
            "search_budget_exhausted" => Some(SwitchRule::SearchBudgetExhausted),
            "cycle_count" => Some(SwitchRule::CycleCount),
            "operator" => Some(SwitchRule::Operator),
            _ => None,
        }
    }
}

/// What a `CompatibilityBroken` regression does next (§5h.8 §6 — the
/// cycle's declared broken-compatibility policy).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokenPolicy {
    /// Re-enter `harness_search` against the drifted base.
    ReSearch,
    /// Roll the lineage head back to the last `verified` base and stop.
    Rollback,
}

impl BrokenPolicy {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            BrokenPolicy::ReSearch => "re_search",
            BrokenPolicy::Rollback => "rollback",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<BrokenPolicy> {
        match s {
            "re_search" => Some(BrokenPolicy::ReSearch),
            "rollback" => Some(BrokenPolicy::Rollback),
            _ => None,
        }
    }
}

/// `CyclePolicy{weight_phase_budget, harness_search_budget, switch_rule,
/// max_cycles, retention_set_ref?, regression_policy, broken_policy}` —
/// the cycle's declared policy (§5h.8 §6; `weight_phase_budget` is the
/// declared *external* budget the harness reports, never spends —
/// `matched_total` is the only budget arithmetic the cycle does).
#[derive(Debug, Clone, PartialEq)]
pub struct CyclePolicy {
    /// The declared external weight-phase budget (`matched_total`
    /// units — the harness records it; the trainer spends it).
    pub weight_phase_budget: u64,
    /// The harness-phase `SearchBudgetRecord` JSON (the ordinary
    /// search-budget shape — `budget/1`).
    pub harness_search_budget: Json,
    /// When the cycle switches phases.
    pub switch_rule: SwitchRule,
    /// The maximum number of cycles.
    pub max_cycles: u32,
    /// The retention set ref the regression suite must not regress.
    pub retention_set_ref: Option<String>,
    /// The regression suite policy (suite pins + margins — JSON; the
    /// recipe's own record).
    pub regression_policy: Option<Json>,
    /// The `CompatibilityBroken` policy.
    pub broken_policy: BrokenPolicy,
}

impl CyclePolicy {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert(
            "weight_phase_budget".into(),
            Json::Int(self.weight_phase_budget as i64),
        );
        m.insert(
            "harness_search_budget".into(),
            self.harness_search_budget.clone(),
        );
        m.insert("switch_rule".into(), Json::str(self.switch_rule.name()));
        m.insert("max_cycles".into(), Json::Int(self.max_cycles as i64));
        if let Some(r) = &self.retention_set_ref {
            m.insert("retention_set_ref".into(), Json::str(r));
        }
        if let Some(r) = &self.regression_policy {
            m.insert("regression_policy".into(), r.clone());
        }
        m.insert("broken_policy".into(), Json::str(self.broken_policy.name()));
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<CyclePolicy, CoEvolutionError> {
        const REC: &str = "CyclePolicy";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "weight_phase_budget",
                "harness_search_budget",
                "switch_rule",
                "max_cycles",
                "retention_set_ref",
                "regression_policy",
                "broken_policy",
            ],
            REC,
        )?;
        Ok(CyclePolicy {
            weight_phase_budget: int_at(m, "weight_phase_budget", REC)? as u64,
            harness_search_budget: member_at(m, "harness_search_budget", REC)?.clone(),
            switch_rule: SwitchRule::parse(str_at(m, "switch_rule", REC)?)
                .ok_or_else(|| SchemaError::v("switch_rule", "unknown switch_rule"))?,
            max_cycles: int_at(m, "max_cycles", REC)? as u32,
            retention_set_ref: opt_str_at(m, "retention_set_ref")?.map(str::to_string),
            regression_policy: m
                .get("regression_policy")
                .filter(|j| !matches!(j, Json::Null))
                .cloned(),
            broken_policy: BrokenPolicy::parse(str_at(m, "broken_policy", REC)?)
                .ok_or_else(|| SchemaError::v("broken_policy", "unknown broken_policy"))?,
        })
    }
}

/// `CycleStopReason` — the closed stop sum (§5h.8 §6):
/// `max_cycles | search_budget_exhausted | veto_tripped |
/// compatibility_broken | consolidation_absorbed | operator{reason}`.
#[derive(Debug, Clone, PartialEq)]
pub enum CycleStopReason {
    /// `max_cycles` reached.
    MaxCycles,
    /// The harness-phase search budget exhausted.
    SearchBudgetExhausted,
    /// A veto fired.
    VetoTripped,
    /// The regression suite broke compatibility (the `broken_policy`
    /// resolved).
    CompatibilityBroken,
    /// The consolidation absorbed the rule (the cycle's declared
    /// end-state).
    ConsolidationAbsorbed,
    /// An operator stop (the reason rides the member).
    Operator {
        /// The stop reason.
        reason: String,
    },
}

impl CycleStopReason {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        match self {
            CycleStopReason::Operator { reason } => {
                Json::obj([("operator", Json::obj([("reason", Json::str(reason))]))])
            }
            other => Json::str(match other {
                CycleStopReason::MaxCycles => "max_cycles",
                CycleStopReason::SearchBudgetExhausted => "search_budget_exhausted",
                CycleStopReason::VetoTripped => "veto_tripped",
                CycleStopReason::CompatibilityBroken => "compatibility_broken",
                CycleStopReason::ConsolidationAbsorbed => "consolidation_absorbed",
                CycleStopReason::Operator { .. } => unreachable!(),
            }),
        }
    }
}

/// `CyclePhaseEntry{phase, inputs, outputs, experiment_ref?,
/// training_run_ref?, verdict?, budgets}` — one phase's sidecar entry
/// (§5h.8 §6; every phase's evidence is the ordinary
/// `ComparisonReport`/experiment vocabulary — no new report kind).
#[derive(Debug, Clone, PartialEq)]
pub struct CyclePhaseEntry {
    /// The phase.
    pub phase: CyclePhase,
    /// The phase's inputs (`{base_ref, export_id?, suite_ref?}`).
    pub inputs: Json,
    /// The phase's outputs (`{candidate_refs[], snapshot_out?, report_refs[]}`).
    pub outputs: Json,
    /// The experiment/campaign ref the phase ran under.
    pub experiment_ref: Option<String>,
    /// The external training-run ref (`weight_update` only — a claim,
    /// never a served address).
    pub training_run_ref: Option<String>,
    /// The phase verdict.
    pub verdict: Option<String>,
    /// The budgets the phase consumed (`{search?, eval?, training?}`).
    pub budgets: Json,
}

impl CyclePhaseEntry {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("phase".into(), Json::str(self.phase.name()));
        m.insert("inputs".into(), self.inputs.clone());
        m.insert("outputs".into(), self.outputs.clone());
        if let Some(e) = &self.experiment_ref {
            m.insert("experiment_ref".into(), Json::str(e));
        }
        if let Some(t) = &self.training_run_ref {
            m.insert("training_run_ref".into(), Json::str(t));
        }
        if let Some(v) = &self.verdict {
            m.insert("verdict".into(), Json::str(v));
        }
        m.insert("budgets".into(), self.budgets.clone());
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<CyclePhaseEntry, CoEvolutionError> {
        const REC: &str = "CyclePhaseEntry";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "phase",
                "inputs",
                "outputs",
                "experiment_ref",
                "training_run_ref",
                "verdict",
                "budgets",
            ],
            REC,
        )?;
        Ok(CyclePhaseEntry {
            phase: CyclePhase::parse(str_at(m, "phase", REC)?)
                .ok_or_else(|| SchemaError::v("phase", "unknown phase"))?,
            inputs: member_at(m, "inputs", REC)?.clone(),
            outputs: member_at(m, "outputs", REC)?.clone(),
            experiment_ref: opt_str_at(m, "experiment_ref")?.map(str::to_string),
            training_run_ref: opt_str_at(m, "training_run_ref")?.map(str::to_string),
            verdict: opt_str_at(m, "verdict")?.map(str::to_string),
            budgets: member_at(m, "budgets", REC)?.clone(),
        })
    }
}

/// `CoEvolutionCycleRecord{cycle_id, lineage_ref, policy, phases[],
/// experiment_refs[], training_run_ref?, search_budget, eval_budget,
/// training_budget, verdict?, stop_reason?}` — the sidecar record
/// (§5h.8 §6; `cycle_id = version_id`, `maturity = research-grade`,
/// `preview`-labelled; the record lives beside the campaign's bundle —
/// it never opens a run of its own).
#[derive(Debug, Clone, PartialEq)]
pub struct CoEvolutionCycleRecord {
    /// The cycle's content id (`cycle_id = version_id` — `seal` fills).
    pub cycle_id: String,
    /// The lineage the cycle operates on.
    pub lineage_ref: String,
    /// The cycle's policy.
    pub policy: CyclePolicy,
    /// The phase entries, in order.
    pub phases: Vec<CyclePhaseEntry>,
    /// Every experiment/campaign ref the cycle cites.
    pub experiment_refs: Vec<String>,
    /// The external training-run ref the weight phase cites.
    pub training_run_ref: Option<String>,
    /// The matched-total budget the cycle reports
    /// (`{search, eval, training}` — `matched_total` arithmetic only;
    /// the weight budget is *declared*, the harness budgets are
    /// *charged*).
    pub budgets: Json,
    /// The cycle verdict, once it stops.
    pub verdict: Option<String>,
    /// The stop reason, once it stops.
    pub stop_reason: Option<CycleStopReason>,
}

impl CoEvolutionCycleRecord {
    /// The canonical JSON (with the `co_evolution_cycle` member +
    /// maturity/preview stamps).
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("co_evolution_cycle".into(), Json::str("1"));
        m.insert("cycle_id".into(), Json::str(&self.cycle_id));
        m.insert("lineage_ref".into(), Json::str(&self.lineage_ref));
        m.insert("policy".into(), self.policy.to_json());
        m.insert(
            "phases".into(),
            Json::Arr(self.phases.iter().map(CyclePhaseEntry::to_json).collect()),
        );
        m.insert(
            "experiment_refs".into(),
            Json::Arr(self.experiment_refs.iter().map(Json::str).collect()),
        );
        if let Some(t) = &self.training_run_ref {
            m.insert("training_run_ref".into(), Json::str(t));
        }
        m.insert("budgets".into(), self.budgets.clone());
        if let Some(v) = &self.verdict {
            m.insert("verdict".into(), Json::str(v));
        }
        if let Some(s) = &self.stop_reason {
            m.insert("stop_reason".into(), s.to_json());
        }
        m.insert("maturity".into(), Json::str(MATURITY));
        m.insert("label".into(), Json::str(PREVIEW_LABEL));
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<CoEvolutionCycleRecord, CoEvolutionError> {
        const REC: &str = "CoEvolutionCycleRecord";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "co_evolution_cycle",
                "cycle_id",
                "lineage_ref",
                "policy",
                "phases",
                "experiment_refs",
                "training_run_ref",
                "budgets",
                "verdict",
                "stop_reason",
                "maturity",
                "label",
            ],
            REC,
        )?;
        let phases = enum_vec_at(m, "phases", REC, |j| CyclePhaseEntry::from_json(j).ok())?;
        Ok(CoEvolutionCycleRecord {
            cycle_id: str_at(m, "cycle_id", REC)?.to_string(),
            lineage_ref: str_at(m, "lineage_ref", REC)?.to_string(),
            policy: CyclePolicy::from_json(member_at(m, "policy", REC)?)?,
            phases,
            experiment_refs: str_vec_at(m, "experiment_refs", REC)?,
            training_run_ref: opt_str_at(m, "training_run_ref")?.map(str::to_string),
            budgets: member_at(m, "budgets", REC)?.clone(),
            verdict: opt_str_at(m, "verdict")?.map(str::to_string),
            stop_reason: m.get("stop_reason").and_then(|s| match s {
                Json::Str(sp) => match sp.as_str() {
                    "max_cycles" => Some(CycleStopReason::MaxCycles),
                    "search_budget_exhausted" => Some(CycleStopReason::SearchBudgetExhausted),
                    "veto_tripped" => Some(CycleStopReason::VetoTripped),
                    "compatibility_broken" => Some(CycleStopReason::CompatibilityBroken),
                    "consolidation_absorbed" => Some(CycleStopReason::ConsolidationAbsorbed),
                    _ => None,
                },
                Json::Obj(o) => o.get("operator").and_then(|oo| {
                    oo.get("reason")
                        .and_then(Json::as_str)
                        .map(|r| CycleStopReason::Operator {
                            reason: r.to_string(),
                        })
                }),
                _ => None,
            }),
        })
    }

    /// Fill `cycle_id` with the body's `idp/1` address (`version_id`
    /// content-derived).
    pub fn seal(&mut self) {
        self.cycle_id = String::new();
        self.cycle_id = hh_identity::idp_id(
            "hh.co_evolution_cycle",
            &self.to_json().to_canonical_string().into_bytes(),
        );
    }

    /// The completed phases' count (the `max_cycles` leg's numerator —
    /// one "cycle" is one full `harness_search → consolidation` lap;
    /// the record counts completed `consolidation` phases).
    pub fn cycles_completed(&self) -> u32 {
        self.phases
            .iter()
            .filter(|p| p.phase == CyclePhase::Consolidation)
            .count() as u32
    }
}

/// `cycle_open(policy, lineage_ref)` — mint the cycle's sidecar record
/// (the driver's `open` fold — pure; the caller deposits the record).
pub fn cycle_open(policy: CyclePolicy, lineage_ref: &str) -> CoEvolutionCycleRecord {
    let mut r = CoEvolutionCycleRecord {
        cycle_id: String::new(),
        lineage_ref: lineage_ref.to_string(),
        policy,
        phases: Vec::new(),
        experiment_refs: Vec::new(),
        training_run_ref: None,
        budgets: Json::obj([
            ("search", Json::Int(0)),
            ("eval", Json::Int(0)),
            ("training", Json::Int(0)),
        ]),
        verdict: None,
        stop_reason: None,
    };
    r.seal();
    r
}

/// `cycle_begin_phase(record, phase, inputs)` — append a phase-open
/// entry (the phase's outputs/`verdict` fill at `complete_phase`; the
/// phase sum is the closed four-member set — `IllegalPhase` on any
/// other spelling is the caller's decode refusal).
pub fn cycle_begin_phase(
    record: &mut CoEvolutionCycleRecord,
    phase: CyclePhase,
    inputs: Json,
) -> Result<(), CoEvolutionError> {
    if record.stop_reason.is_some() {
        return Err(CoEvolutionError::IllegalPhase {
            detail: "the cycle has stopped".into(),
        });
    }
    record.phases.push(CyclePhaseEntry {
        phase,
        inputs,
        outputs: Json::obj([]),
        experiment_ref: None,
        training_run_ref: None,
        verdict: None,
        budgets: Json::obj([]),
    });
    record.seal();
    Ok(())
}

/// `cycle_complete_phase(record, outputs, experiment_ref?,
/// training_run_ref?, verdict?, budgets)` — fill the open phase's
/// outputs (the last entry's `outputs` are still empty; completing a
/// phase with no open entry is `IllegalPhase`).
pub fn cycle_complete_phase(
    record: &mut CoEvolutionCycleRecord,
    outputs: Json,
    experiment_ref: Option<&str>,
    training_run_ref: Option<&str>,
    verdict: Option<&str>,
    budgets: Json,
) -> Result<(), CoEvolutionError> {
    let entry = record
        .phases
        .iter_mut()
        .rev()
        .find(|p| p.outputs == Json::obj([]))
        .ok_or_else(|| CoEvolutionError::IllegalPhase {
            detail: "no open phase to complete".into(),
        })?;
    entry.outputs = outputs;
    entry.experiment_ref = experiment_ref.map(str::to_string);
    entry.training_run_ref = training_run_ref.map(str::to_string);
    entry.verdict = verdict.map(str::to_string);
    entry.budgets = budgets.clone();
    if let Some(e) = experiment_ref {
        if !record.experiment_refs.iter().any(|x| x == e) {
            record.experiment_refs.push(e.to_string());
        }
    }
    if let Some(t) = training_run_ref {
        record.training_run_ref = Some(t.to_string());
    }
    // matched_total arithmetic — the declared budgets add, never split.
    if let (Json::Obj(total), Json::Obj(phase)) = (&mut record.budgets, &budgets) {
        for k in ["search", "eval", "training"] {
            if let Some(v) = phase.get(k).and_then(Json::as_int) {
                let cur = total.get(k).and_then(Json::as_int).unwrap_or(0);
                total.insert(k.to_string(), Json::Int(cur + v));
            }
        }
    }
    record.seal();
    Ok(())
}

/// `PhasePlan` — what the cycle does next (the driver's
/// `next_phase` output; a plan, never an opened run).
#[derive(Debug, Clone, PartialEq)]
pub enum PhasePlan {
    /// Enter `harness_search` against `base_ref` (an ordinary campaign).
    HarnessSearch {
        /// The base the search binds.
        base_ref: String,
    },
    /// Enter `weight_update` — export under the policy, train
    /// externally, import the claim.
    WeightUpdate,
    /// Enter `re_evaluation` — run the exported regression suite.
    ReEvaluation,
    /// Enter `consolidation`.
    Consolidation,
    /// Stop.
    Stop {
        /// The stop reason.
        reason: CycleStopReason,
    },
    /// `compatibility_broken` resolved per `broken_policy` — re-enter
    /// `harness_search` (the `re_search` leg).
    ReSearch {
        /// The drifted base the re-search binds.
        base_ref: String,
    },
    /// `compatibility_broken` resolved per `broken_policy` — roll the
    /// head back and stop (the `rollback` leg).
    Rollback {
        /// The last `verified` base the head restores.
        base_ref: String,
    },
}

/// `cycle_next(record)` — the deterministic phase planner (§5h.8 §6):
///
/// - `stop_reason` set → `Stop` (the terminal fold).
/// - `cycles_completed ≥ max_cycles` → `Stop{MaxCycles}`.
/// - the last completed `re_evaluation` phase carried
///   `verdict = broken` → `broken_policy` resolves (`ReSearch` /
///   `Rollback`, the base the phase's inputs name).
/// - otherwise the next phase in the order wraps — `harness_search`
///   begins a new lap when `consolidation` just completed.
pub fn cycle_next(record: &CoEvolutionCycleRecord) -> PhasePlan {
    if let Some(r) = &record.stop_reason {
        return PhasePlan::Stop { reason: r.clone() };
    }
    if record.cycles_completed() >= record.policy.max_cycles {
        return PhasePlan::Stop {
            reason: CycleStopReason::MaxCycles,
        };
    }
    // The last phase decides the next step.
    let last = record.phases.last();
    match last.map(|p| p.phase) {
        None | Some(CyclePhase::Consolidation) => {
            let base = last
                .and_then(|p| p.outputs.get("base_ref"))
                .and_then(Json::as_str)
                .or_else(|| {
                    record
                        .phases
                        .iter()
                        .rev()
                        .find_map(|p| p.inputs.get("base_ref").and_then(Json::as_str))
                })
                .unwrap_or("")
                .to_string();
            PhasePlan::HarnessSearch { base_ref: base }
        }
        Some(CyclePhase::HarnessSearch) => PhasePlan::WeightUpdate,
        Some(CyclePhase::WeightUpdate) => PhasePlan::ReEvaluation,
        Some(CyclePhase::ReEvaluation) => {
            let broken = last
                .and_then(|p| p.verdict.as_deref())
                .map(|v| v == "broken" || v == "compatibility_broken")
                .unwrap_or(false);
            if broken {
                let base = last
                    .and_then(|p| p.inputs.get("base_ref").and_then(Json::as_str))
                    .unwrap_or("")
                    .to_string();
                match record.policy.broken_policy {
                    BrokenPolicy::ReSearch => PhasePlan::ReSearch { base_ref: base },
                    BrokenPolicy::Rollback => PhasePlan::Rollback { base_ref: base },
                }
            } else {
                PhasePlan::Consolidation
            }
        }
    }
}

/// `cycle_stop(record, reason, verdict)` — the terminal fold (the
/// record seals its `stop_reason`/`verdict`; the caller deposits).
pub fn cycle_stop(
    record: &mut CoEvolutionCycleRecord,
    reason: CycleStopReason,
    verdict: Option<&str>,
) {
    record.stop_reason = Some(reason);
    record.verdict = verdict.map(str::to_string);
    record.seal();
}

// ── The null trainer — the AC-executable fixture (§5h.8 §7) ───────────────

/// `NullTrainer` — the conformance fixture (§5h.8 §7): a trainer that
/// *returns identical weights under a new snapshot id*. Export mode
/// produces a `SnapshotClaim` (`trained_under` the caller supplies,
/// `weights_digest` = the base's claim — identical weights is data,
/// never a served address); attached mode serves a `policy_version`
/// counter (`bump()` exercises the version-pin leg).
///
/// The null trainer exists so the full cycle is executable without a
/// real trainer — it never touches bytes of any weights.
#[derive(Debug, Clone, PartialEq)]
pub struct NullTrainer {
    /// The fixture's seed (the snapshot-id namespace — `null/<seed>/<n>`).
    pub seed: String,
    /// The attached-mode policy-version counter.
    pub policy_version: u64,
    /// The number of `train` calls the fixture served.
    pub trains: u64,
}

impl NullTrainer {
    /// A fresh fixture.
    pub fn new(seed: &str) -> NullTrainer {
        NullTrainer {
            seed: seed.to_string(),
            policy_version: 1,
            trains: 0,
        }
    }

    /// The attached-mode serving id (`<model>-pv<counter>` — the
    /// `policy_version` pin a `SnapshotClaim` carries).
    pub fn serve_snapshot_id(&self, model_id: &str) -> String {
        format!("{model_id}-pv{}", self.policy_version)
    }

    /// Bump the attached-mode policy-version counter (the drift leg —
    /// `import_snapshot` sees the new `policy_version` claim).
    pub fn bump_policy_version(&mut self) {
        self.policy_version += 1;
    }

    /// `train(export_id, base_claim, trained_under)` — the
    /// export-mode leg: identical weights (`weights_digest` =
    /// `base.weights_digest`, `method = "null_trainer"`), a new
    /// `snapshot_id`, `policy_version_exposed = supported`, the
    /// `TrainingLineage` naming the export + the fixture run. The claim
    /// carries no provenance member — the import's provenance is
    /// records-in at [`import_snapshot`].
    pub fn train(
        &mut self,
        export_id: &str,
        base: &SnapshotClaim,
        trained_under: Vec<TrainedUnderRef>,
    ) -> SnapshotClaim {
        self.trains += 1;
        let snapshot_id = format!("null/{}/{:04}", self.seed, self.trains);
        SnapshotClaim {
            provider: base.provider.clone(),
            model_id: base.model_id.clone(),
            snapshot_id,
            serving_route: None,
            base_snapshot_ref: Some(base.snapshot_id.clone()),
            training_lineage: Some(TrainingLineage {
                training_run_ref: format!("null_trainer_run:{}", self.trains),
                step: 0,
                data_refs: vec![export_id.to_string()],
                method: "null_trainer".to_string(),
                compute_claim: crate::model::ComputeClaim {
                    descriptor: Json::obj([]),
                },
            }),
            trained_under,
            training_cutoff_claim: base.training_cutoff_claim.clone(),
            weights_digest: base.weights_digest.clone(),
            policy_version_exposed: PolicyVersionExposed::Supported,
        }
    }
}
