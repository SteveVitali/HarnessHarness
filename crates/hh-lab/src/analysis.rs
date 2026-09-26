//! The analysis records (spec §6.4; R-2.10.4⁰ᵃ; S1.24;
//! ADR-0157…0160, ADR-0046 as amended).
//!
//! Records: [`AnalysisSpec`], [`AnalysisRecord`], [`AnalysisReport`],
//! [`QuerySpec`], [`WatermarkSet`], [`CellRecord`], [`ResamplePlan`],
//! [`RenderSpec`], [`ReportLabel`], and the amended [`ComparisonReport`]
//! (with [`PairedEffect`], [`BudgetMatch`], [`TestRecord`], [`SignProfile`],
//! [`TailEffects`], [`OutcomeBounds`], [`Multiplicity`], [`BenefitKind`],
//! [`TestKind`], [`BudgetMatchStatus`], [`ReportLabelKind`]).
//!
//! [`ParityReport`] — the §5h.4 import parity report — lives here because it
//! *is* a `ComparisonReport{benefit_kind = artifact_benefit}` plus the replay
//! members; `bench::SuiteManifest` references it.

use std::collections::BTreeMap;

use hh_identity::idp::identify_bytes;
use hh_identity::kinds::RecordKind;
use hh_ontology::eval::{EstimatorSelection, IntervalMethod};
use hh_wire::Json;

use crate::json_util::*;

// ── ResamplePlan ────────────────────────────────────────────────────────────

/// `ResampleKind ∈ {iid, bootstrap, block_bootstrap, permutation}` — the
/// resample plans (§2.2's estimator vocabulary, §6.4's plan member).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResampleKind {
    /// IID resampling.
    Iid,
    /// Bootstrap over replicates.
    Bootstrap,
    /// Block bootstrap (paired-by-task blocks).
    BlockBootstrap,
    /// A permutation test.
    Permutation,
}

impl ResampleKind {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            ResampleKind::Iid => "iid",
            ResampleKind::Bootstrap => "bootstrap",
            ResampleKind::BlockBootstrap => "block_bootstrap",
            ResampleKind::Permutation => "permutation",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<ResampleKind> {
        match s {
            "iid" => Some(ResampleKind::Iid),
            "bootstrap" => Some(ResampleKind::Bootstrap),
            "block_bootstrap" => Some(ResampleKind::BlockBootstrap),
            "permutation" => Some(ResampleKind::Permutation),
            _ => None,
        }
    }
}

/// `ResamplePlan{kind, replicates, seed?, block_size?}` — the analysis's
/// resample declaration (§6.4; "as in 2.2").
#[derive(Debug, Clone, PartialEq)]
pub struct ResamplePlan {
    /// The resample kind.
    pub kind: ResampleKind,
    /// The replicate count.
    pub replicates: u32,
    /// The resample seed.
    pub seed: Option<String>,
    /// The block size (`block_bootstrap` only).
    pub block_size: Option<u32>,
}

impl ResamplePlan {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("kind".into(), Json::str(self.kind.name()));
        m.insert("replicates".into(), Json::Int(self.replicates as i64));
        if let Some(s) = &self.seed {
            m.insert("seed".into(), Json::str(s));
        }
        if let Some(b) = self.block_size {
            m.insert("block_size".into(), Json::Int(b as i64));
        }
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<ResamplePlan, SchemaError> {
        const REC: &str = "ResamplePlan";
        let m = expect_obj(j, REC)?;
        reject_unknown(m, &["kind", "replicates", "seed", "block_size"], REC)?;
        Ok(ResamplePlan {
            kind: ResampleKind::parse(str_at(m, "kind", REC)?)
                .ok_or_else(|| SchemaError::v("kind", "unknown resample kind"))?,
            replicates: int_at(m, "replicates", REC)? as u32,
            seed: opt_str_at(m, "seed")?.map(str::to_string),
            block_size: opt_int_at(m, "block_size")?.map(|v| v as u32),
        })
    }
}

// ── QuerySpec / WatermarkSet ────────────────────────────────────────────────

/// `QuerySpec{metrics, filters?, grain?}` — the analysis's selection (§6.4).
#[derive(Debug, Clone, PartialEq)]
pub struct QuerySpec {
    /// The metrics the analysis computes.
    pub metrics: Vec<String>,
    /// Row filters (schema-opaque at C0).
    pub filters: Option<Json>,
    /// The reporting grain (`per_task | per_cell | per_arm`).
    pub grain: Option<String>,
}

impl QuerySpec {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert(
            "metrics".into(),
            Json::Arr(self.metrics.iter().map(Json::str).collect()),
        );
        if let Some(f) = &self.filters {
            m.insert("filters".into(), f.clone());
        }
        if let Some(g) = &self.grain {
            m.insert("grain".into(), Json::str(g));
        }
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<QuerySpec, SchemaError> {
        const REC: &str = "QuerySpec";
        let m = expect_obj(j, REC)?;
        reject_unknown(m, &["metrics", "filters", "grain"], REC)?;
        Ok(QuerySpec {
            metrics: str_vec_at(m, "metrics", REC)?,
            filters: m
                .get("filters")
                .filter(|j| !matches!(j, Json::Null))
                .cloned(),
            grain: opt_str_at(m, "grain")?.map(str::to_string),
        })
    }
}

/// `watermark_set` — `map<store, watermark>`: the store watermarks the
/// analysis was generated from (§6.4; provenance for the input set).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WatermarkSet {
    /// `store → watermark`.
    pub watermarks: BTreeMap<String, String>,
}

impl WatermarkSet {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::Obj(
            self.watermarks
                .iter()
                .map(|(k, v)| (k.clone(), Json::str(v)))
                .collect(),
        )
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<WatermarkSet, SchemaError> {
        let m = expect_obj(j, "WatermarkSet")?;
        let mut watermarks = BTreeMap::new();
        for (k, v) in m {
            watermarks.insert(
                k.clone(),
                v.as_str()
                    .ok_or_else(|| SchemaError::v(k.clone(), "watermark must be a string"))?
                    .to_string(),
            );
        }
        Ok(WatermarkSet { watermarks })
    }
}

// ── AnalysisSpec / AnalysisRecord / AnalysisReport ──────────────────────────

/// `AnalysisSpec` — what the analysis computes (§6.4). The `kind` /
/// `spec_ref` / `label` members were added at S3.4c (CC8 additive; absent
/// decodes to `summarize` / `None` / `None` — the only shape the pre-S3.4c
/// schema could express was the A1 record).
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisSpec {
    /// The spec's id (content-addressed — `analysis_record` domain).
    pub spec_id: String,
    /// The analysis kind — `summarize` | `compare` | `interaction` |
    /// `transfer` | `equivalence` at C0/Stage 3 (the A1/A2/A3-contrast/A8 +
    /// transfer-row set; A12 `multiplicity` rides every comparison report,
    /// never a standalone kind).
    pub kind: String,
    /// The selection.
    pub query: QuerySpec,
    /// The experiment/design spec ref this analysis reads (a LabDocs
    /// `experiment` doc id — resolves `Design`, arms and pre-registration).
    /// `None` for design-free `summarize` analyses.
    pub spec_ref: Option<String>,
    /// The requested report label (`confirmatory` requires a matching
    /// pre-registration ref — ADR-0157 D5).
    pub label: Option<String>,
    /// The estimator selection (the CF-337 record — `declared`, `floors`,
    /// `fallback_chain`, `substituted?`).
    pub estimator_selection: EstimatorSelection,
    /// The resample plan.
    pub resample: Option<ResamplePlan>,
    /// The declared outputs (`report | cells | comparison | render`).
    pub outputs: Vec<String>,
}

impl AnalysisSpec {
    /// `spec_id = H(canonical(spec minus spec_id))` under `analysis_record`.
    pub fn spec_id(&self) -> String {
        let mut j = self.to_json();
        if let Json::Obj(ref mut m) = j {
            m.remove("spec_id");
        }
        identify_bytes(
            RecordKind::AnalysisRecord,
            j.to_canonical_string().as_bytes(),
        )
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("spec_id".into(), Json::str(&self.spec_id));
        m.insert("kind".into(), Json::str(&self.kind));
        m.insert("query".into(), self.query.to_json());
        if let Some(r) = &self.spec_ref {
            m.insert("spec_ref".into(), Json::str(r));
        }
        if let Some(l) = &self.label {
            m.insert("label".into(), Json::str(l));
        }
        m.insert(
            "estimator_selection".into(),
            self.estimator_selection.to_json(),
        );
        if let Some(r) = &self.resample {
            m.insert("resample".into(), r.to_json());
        }
        m.insert(
            "outputs".into(),
            Json::Arr(self.outputs.iter().map(Json::str).collect()),
        );
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<AnalysisSpec, SchemaError> {
        const REC: &str = "AnalysisSpec";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "spec_id",
                "kind",
                "query",
                "spec_ref",
                "label",
                "estimator_selection",
                "resample",
                "outputs",
            ],
            REC,
        )?;
        Ok(AnalysisSpec {
            spec_id: str_at(m, "spec_id", REC)?.to_string(),
            kind: opt_str_at(m, "kind")?
                .map(str::to_string)
                .unwrap_or_else(|| "summarize".to_string()),
            query: QuerySpec::from_json(member_at(m, "query", REC)?)?,
            spec_ref: opt_str_at(m, "spec_ref")?.map(str::to_string),
            label: opt_str_at(m, "label")?.map(str::to_string),
            estimator_selection: EstimatorSelection::from_json(member_at(
                m,
                "estimator_selection",
                REC,
            )?)
            .map_err(|e| SchemaError::v("estimator_selection", format!("{e:?}")))?,
            resample: match m.get("resample") {
                None | Some(Json::Null) => None,
                Some(r) => Some(ResamplePlan::from_json(r)?),
            },
            outputs: str_vec_at(m, "outputs", REC)?,
        })
    }
}

/// `AnalysisStatus ∈ {final, exploratory}` — an exploratory analysis is
/// labelled, never headlined (ADR-0157).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnalysisStatus {
    /// The analysis is final.
    Final,
    /// The analysis is exploratory (never the sole basis of acceptance).
    Exploratory,
}

impl AnalysisStatus {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            AnalysisStatus::Final => "final",
            AnalysisStatus::Exploratory => "exploratory",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<AnalysisStatus> {
        match s {
            "final" => Some(AnalysisStatus::Final),
            "exploratory" => Some(AnalysisStatus::Exploratory),
            _ => None,
        }
    }
}

/// `AnalysisRecord` — the persisted analysis (§6.4 §6.5; content-addressed
/// under `analysis_record`). The C1 canonical members are additive — a
/// record produced at C0 decodes with `None`/empty/false on every C1 member.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisRecord {
    /// `analysis_id = H(canonical(record minus analysis_id))`.
    pub analysis_id: String,
    /// The spec this record ran.
    pub spec_ref: String,
    /// The input watermarks.
    pub generated_from: WatermarkSet,
    /// The produced output refs.
    pub outputs: Vec<String>,
    /// The record status.
    pub status: AnalysisStatus,
    /// The record kind (`summary_report` | `comparison_report` |
    /// `interaction_report` | `frontier_report` | `transfer_report` |
    /// `benefit_decomposition` | `rank_report` | `reliability_profile` |
    /// `power_report` | `strata_view` | `diagnostics` | `fitted_surface` |
    /// `equivalence_report` | `custom`; §6.5 producer-contract vocabulary).
    /// `None` on C0-produced records — emit `custom` when re-declared.
    pub kind: Option<String>,
    /// The producing experiment run (None for ad-hoc analyses).
    pub experiment_run_id: Option<String>,
    /// The estimator procedure version_id (what ran, not a name).
    pub procedure: Option<String>,
    /// The declared inputs — `{row_versions[] | {query, watermark_set}}`.
    pub inputs: Option<Json>,
    /// The metric-registry version the analysis resolved against.
    pub metric_registry_version: Option<String>,
    /// The price-table version the analysis resolved against.
    pub price_table_version: Option<String>,
    /// The oracle refs used across the analysis's cells.
    pub oracle_ids: Vec<String>,
    /// The judge snapshot refs used across the analysis's cells.
    pub judge_snapshots: Vec<String>,
    /// The `AnalysisReport` content address the record points at.
    pub report: Option<String>,
    /// `true` when the record's {kind, metric refs, procedure} match a
    /// `PreRegistration` plan by identity (emitted only when `true`).
    pub pre_registered: bool,
    /// The matching registered analysis ref when `pre_registered`.
    pub registered_analysis_ref: Option<String>,
    /// `true` when the record postdates an experiment amendment (emitted
    /// only when `true`).
    pub post_amendment: bool,
    /// The record's provenance payload.
    pub provenance: Option<Json>,
    /// The analysis's own cost account (the compute-effort proxy at this
    /// tier: `{resampling_draws, seed, n_rows, n_tasks}` — ADR-0294).
    pub cost: Option<Json>,
}

impl AnalysisRecord {
    /// `analysis_id = H(canonical(record minus analysis_id))` under
    /// `analysis_record`.
    pub fn analysis_id(&self) -> String {
        let mut j = self.to_json();
        if let Json::Obj(ref mut m) = j {
            m.remove("analysis_id");
        }
        identify_bytes(
            RecordKind::AnalysisRecord,
            j.to_canonical_string().as_bytes(),
        )
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("analysis_id".into(), Json::str(&self.analysis_id));
        m.insert("spec_ref".into(), Json::str(&self.spec_ref));
        m.insert("generated_from".into(), self.generated_from.to_json());
        m.insert(
            "outputs".into(),
            Json::Arr(self.outputs.iter().map(Json::str).collect()),
        );
        m.insert("status".into(), Json::str(self.status.name()));
        if let Some(k) = &self.kind {
            m.insert("kind".into(), Json::str(k));
        }
        if let Some(r) = &self.experiment_run_id {
            m.insert("experiment_run_id".into(), Json::str(r));
        }
        if let Some(p) = &self.procedure {
            m.insert("procedure".into(), Json::str(p));
        }
        if let Some(i) = &self.inputs {
            m.insert("inputs".into(), i.clone());
        }
        if let Some(v) = &self.metric_registry_version {
            m.insert("metric_registry_version".into(), Json::str(v));
        }
        if let Some(v) = &self.price_table_version {
            m.insert("price_table_version".into(), Json::str(v));
        }
        if !self.oracle_ids.is_empty() {
            m.insert(
                "oracle_ids".into(),
                Json::Arr(self.oracle_ids.iter().map(Json::str).collect()),
            );
        }
        if !self.judge_snapshots.is_empty() {
            m.insert(
                "judge_snapshots".into(),
                Json::Arr(self.judge_snapshots.iter().map(Json::str).collect()),
            );
        }
        if let Some(r) = &self.report {
            m.insert("report".into(), Json::str(r));
        }
        if self.pre_registered {
            m.insert("pre_registered".into(), Json::Bool(true));
        }
        if let Some(r) = &self.registered_analysis_ref {
            m.insert("registered_analysis_ref".into(), Json::str(r));
        }
        if self.post_amendment {
            m.insert("post_amendment".into(), Json::Bool(true));
        }
        if let Some(p) = &self.provenance {
            m.insert("provenance".into(), p.clone());
        }
        if let Some(c) = &self.cost {
            m.insert("cost".into(), c.clone());
        }
        Json::Obj(m)
    }

    /// Strict decode (C0 records decode with `None`/`[]`/`false` on every
    /// C1 member — additive per CC8).
    pub fn from_json(j: &Json) -> Result<AnalysisRecord, SchemaError> {
        const REC: &str = "AnalysisRecord";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "analysis_id",
                "spec_ref",
                "generated_from",
                "outputs",
                "status",
                "kind",
                "experiment_run_id",
                "procedure",
                "inputs",
                "metric_registry_version",
                "price_table_version",
                "oracle_ids",
                "judge_snapshots",
                "report",
                "pre_registered",
                "registered_analysis_ref",
                "post_amendment",
                "provenance",
                "cost",
            ],
            REC,
        )?;
        Ok(AnalysisRecord {
            analysis_id: str_at(m, "analysis_id", REC)?.to_string(),
            spec_ref: str_at(m, "spec_ref", REC)?.to_string(),
            generated_from: WatermarkSet::from_json(member_at(m, "generated_from", REC)?)?,
            outputs: str_vec_at(m, "outputs", REC)?,
            status: AnalysisStatus::parse(str_at(m, "status", REC)?)
                .ok_or_else(|| SchemaError::v("status", "unknown analysis status"))?,
            kind: opt_str_at(m, "kind")?.map(str::to_string),
            experiment_run_id: opt_str_at(m, "experiment_run_id")?.map(str::to_string),
            procedure: opt_str_at(m, "procedure")?.map(str::to_string),
            inputs: match m.get("inputs") {
                None | Some(Json::Null) => None,
                Some(i) => Some(i.clone()),
            },
            metric_registry_version: opt_str_at(m, "metric_registry_version")?
                .map(str::to_string),
            price_table_version: opt_str_at(m, "price_table_version")?.map(str::to_string),
            oracle_ids: match m.get("oracle_ids") {
                None | Some(Json::Null) => Vec::new(),
                Some(Json::Arr(a)) => a
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_string)
                            .ok_or_else(|| SchemaError::v("oracle_ids", "expected strings"))
                    })
                    .collect::<Result<_, _>>()?,
                Some(_) => return Err(SchemaError::v("oracle_ids", "expected array")),
            },
            judge_snapshots: match m.get("judge_snapshots") {
                None | Some(Json::Null) => Vec::new(),
                Some(Json::Arr(a)) => a
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_string)
                            .ok_or_else(|| SchemaError::v("judge_snapshots", "expected strings"))
                    })
                    .collect::<Result<_, _>>()?,
                Some(_) => return Err(SchemaError::v("judge_snapshots", "expected array")),
            },
            report: opt_str_at(m, "report")?.map(str::to_string),
            pre_registered: opt_bool_at(m, "pre_registered")?.unwrap_or(false),
            registered_analysis_ref: opt_str_at(m, "registered_analysis_ref")?
                .map(str::to_string),
            post_amendment: opt_bool_at(m, "post_amendment")?.unwrap_or(false),
            provenance: match m.get("provenance") {
                None | Some(Json::Null) => None,
                Some(p) => Some(p.clone()),
            },
            cost: match m.get("cost") {
                None | Some(Json::Null) => None,
                Some(c) => Some(c.clone()),
            },
        })
    }
}

/// `AnalysisReport` — the rendered analysis output (§6.4). `kind` is the
/// additive C1 member (the report's operation kind spelling).
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisReport {
    /// The report id.
    pub report_id: String,
    /// The spec hash the report was generated from.
    pub spec_hash: String,
    /// The input watermarks.
    pub generated_from: WatermarkSet,
    /// The report status.
    pub status: AnalysisStatus,
    /// The result payload ref.
    pub result_ref: Option<String>,
    /// The report kind (`frontier_report` | `rank_report` | …) — additive
    /// C1 member, absent on C0 reports.
    pub kind: Option<String>,
}

impl AnalysisReport {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("report_id".into(), Json::str(&self.report_id));
        m.insert("spec_hash".into(), Json::str(&self.spec_hash));
        m.insert("generated_from".into(), self.generated_from.to_json());
        m.insert("status".into(), Json::str(self.status.name()));
        if let Some(r) = &self.result_ref {
            m.insert("result_ref".into(), Json::str(r));
        }
        if let Some(k) = &self.kind {
            m.insert("kind".into(), Json::str(k));
        }
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<AnalysisReport, SchemaError> {
        const REC: &str = "AnalysisReport";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "report_id",
                "spec_hash",
                "generated_from",
                "status",
                "result_ref",
                "kind",
            ],
            REC,
        )?;
        Ok(AnalysisReport {
            report_id: str_at(m, "report_id", REC)?.to_string(),
            spec_hash: str_at(m, "spec_hash", REC)?.to_string(),
            generated_from: WatermarkSet::from_json(member_at(m, "generated_from", REC)?)?,
            status: AnalysisStatus::parse(str_at(m, "status", REC)?)
                .ok_or_else(|| SchemaError::v("status", "unknown analysis status"))?,
            result_ref: opt_str_at(m, "result_ref")?.map(str::to_string),
            kind: opt_str_at(m, "kind")?.map(str::to_string),
        })
    }
}

/// `Cell/1` — the store's analysis cell record (§6.4; the results-store
/// cell, distinct from `hh_ontology::eval::Cell`'s `(Arm, Configuration,
/// Task)` triple).
#[derive(Debug, Clone, PartialEq)]
pub struct CellRecord {
    /// The cell id.
    pub cell_id: String,
    /// The cell's configuration version id.
    pub configuration_version_id: String,
    /// The arm id.
    pub arm_id: String,
    /// The task ref.
    pub task_ref: String,
    /// The replicate count in this cell.
    pub replicate_count: u32,
}

impl CellRecord {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("cell_id", Json::str(&self.cell_id)),
            (
                "configuration_version_id",
                Json::str(&self.configuration_version_id),
            ),
            ("arm_id", Json::str(&self.arm_id)),
            ("task_ref", Json::str(&self.task_ref)),
            ("replicate_count", Json::Int(self.replicate_count as i64)),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<CellRecord, SchemaError> {
        const REC: &str = "CellRecord";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "cell_id",
                "configuration_version_id",
                "arm_id",
                "task_ref",
                "replicate_count",
            ],
            REC,
        )?;
        Ok(CellRecord {
            cell_id: str_at(m, "cell_id", REC)?.to_string(),
            configuration_version_id: str_at(m, "configuration_version_id", REC)?.to_string(),
            arm_id: str_at(m, "arm_id", REC)?.to_string(),
            task_ref: str_at(m, "task_ref", REC)?.to_string(),
            replicate_count: int_at(m, "replicate_count", REC)? as u32,
        })
    }
}

// ── ComparisonReport (ADR-0046 as amended) ──────────────────────────────────

/// `benefit_kind ∈ {artifact_benefit, search_time_benefit, transfer}` — what
/// the comparison measures (§6.4 A2/A5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BenefitKind {
    /// The frozen artifact's benefit (always a `full_set` held-out experiment).
    ArtifactBenefit,
    /// The search-time benefit (adaptive rows only).
    SearchTimeBenefit,
    /// A transfer comparison (A5).
    Transfer,
}

impl BenefitKind {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            BenefitKind::ArtifactBenefit => "artifact_benefit",
            BenefitKind::SearchTimeBenefit => "search_time_benefit",
            BenefitKind::Transfer => "transfer",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<BenefitKind> {
        match s {
            "artifact_benefit" => Some(BenefitKind::ArtifactBenefit),
            "search_time_benefit" => Some(BenefitKind::SearchTimeBenefit),
            "transfer" => Some(BenefitKind::Transfer),
            _ => None,
        }
    }
}

/// `budget_match.status ∈ {matched, imbalanced, unmatched}` (§6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BudgetMatchStatus {
    /// Within tolerance.
    Matched,
    /// Outside tolerance — reported, never hidden.
    Imbalanced,
    /// The budgets could not be matched at all.
    Unmatched,
}

impl BudgetMatchStatus {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            BudgetMatchStatus::Matched => "matched",
            BudgetMatchStatus::Imbalanced => "imbalanced",
            BudgetMatchStatus::Unmatched => "unmatched",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<BudgetMatchStatus> {
        match s {
            "matched" => Some(BudgetMatchStatus::Matched),
            "imbalanced" => Some(BudgetMatchStatus::Imbalanced),
            "unmatched" => Some(BudgetMatchStatus::Unmatched),
            _ => None,
        }
    }
}

/// `budget_match{limits_equal, consumption_imbalance, tolerance, status}` —
/// the comparison's budget-match record (§6.4 A2: status from consumption
/// medians vs `MatchSpec.tolerance`).
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetMatch {
    /// Whether the declared limits were dimension-wise equal.
    pub limits_equal: bool,
    /// The realised consumption imbalance (ppm per dimension; data at C0).
    pub consumption_imbalance: Option<Json>,
    /// The tolerance applied (ppm).
    pub tolerance_ppm: u64,
    /// The verdict.
    pub status: BudgetMatchStatus,
}

/// `paired_effect{point, interval, method}` — the paired Δ estimate (§6.4
/// A2: mean of per-task paired differences at matched budget).
#[derive(Debug, Clone, PartialEq)]
pub struct PairedEffect {
    /// The point estimate (`None` = `n/a` — e.g. Δ_in interval includes 0
    /// for transfer ratios).
    pub point: Option<Json>,
    /// The interval estimate.
    pub interval: Option<Json>,
    /// The interval method (the CF-337 vocabulary).
    pub method: IntervalMethod,
}

/// `test{kind ∈ {permutation_signflip, wilcoxon_signed_rank, paired_bayes}}`
/// (§6.4 A2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TestKind {
    /// A permutation/sign-flip test.
    PermutationSignflip,
    /// A Wilcoxon signed-rank test.
    WilcoxonSignedRank,
    /// A paired Bayesian comparison.
    PairedBayes,
}

impl TestKind {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            TestKind::PermutationSignflip => "permutation_signflip",
            TestKind::WilcoxonSignedRank => "wilcoxon_signed_rank",
            TestKind::PairedBayes => "paired_bayes",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<TestKind> {
        match s {
            "permutation_signflip" => Some(TestKind::PermutationSignflip),
            "wilcoxon_signed_rank" => Some(TestKind::WilcoxonSignedRank),
            "paired_bayes" => Some(TestKind::PairedBayes),
            _ => None,
        }
    }
}

/// `test{kind}` — the comparison's test record.
#[derive(Debug, Clone, PartialEq)]
pub struct TestRecord {
    /// The test kind.
    pub kind: TestKind,
}

/// `sign_profile{helped, hurt, unchanged}` — the per-task sign counts
/// (§6.4 A2).
#[derive(Debug, Clone, PartialEq)]
pub struct SignProfile {
    /// Tasks where arm A beat arm B.
    pub helped: u64,
    /// Tasks where arm B beat arm A.
    pub hurt: u64,
    /// Tasks with no measurable difference.
    pub unchanged: u64,
}

/// `tail_effects{P50, P95, max}` (§6.4 A2).
#[derive(Debug, Clone, PartialEq)]
pub struct TailEffects {
    /// The median tail effect.
    pub p50: Json,
    /// The 95th-percentile tail effect.
    pub p95: Json,
    /// The maximum tail effect.
    pub max: Json,
}

/// `outcome_bounds{lower, upper, verdict ∈ {robust, sensitive}}` (§6.4 A2).
#[derive(Debug, Clone, PartialEq)]
pub struct OutcomeBounds {
    /// The lower bound.
    pub lower: Json,
    /// The upper bound.
    pub upper: Json,
    /// Whether the verdict is robust to the bounds.
    pub verdict: OutcomeBoundsVerdict,
}

/// `outcome_bounds.verdict ∈ {robust, sensitive}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OutcomeBoundsVerdict {
    /// The verdict is robust.
    Robust,
    /// The verdict is sensitive to the bounds.
    Sensitive,
}

impl OutcomeBoundsVerdict {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            OutcomeBoundsVerdict::Robust => "robust",
            OutcomeBoundsVerdict::Sensitive => "sensitive",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<OutcomeBoundsVerdict> {
        match s {
            "robust" => Some(OutcomeBoundsVerdict::Robust),
            "sensitive" => Some(OutcomeBoundsVerdict::Sensitive),
            _ => None,
        }
    }
}

/// `multiplicity{family, raw?, adjusted, label?}` — the family-size and
/// correction record (§6.4 A2/A12). `raw_ppm`/`adjusted_ppm`/`label` were
/// added at S3.4c (CC8 additive; absent on pre-S3.4c encodings) so A12's
/// Holm/BH correction is a report fact, never a side-channel.
#[derive(Debug, Clone, PartialEq)]
pub struct Multiplicity {
    /// The family size (number of comparisons the correction covers).
    pub family_size: u32,
    /// The correction method (`holm`, `benjamini_hochberg`, `none`, …).
    pub adjusted: String,
    /// The raw sign-flip permutation p-value (ppm) before correction.
    pub raw_ppm: Option<i64>,
    /// The corrected p/q-value (ppm) after `adjusted`.
    pub adjusted_ppm: Option<i64>,
    /// The cell's multiplicity label (`confirmatory` | `exploratory`).
    pub label: Option<String>,
}

/// `label` — the report's label (ADR-0157 D4–D6: every "X beats Y" is a
/// `ComparisonReport` *with a label*).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReportLabelKind {
    /// A confirmatory, headline-eligible report.
    Headlined,
    /// A descriptive report — not headline-eligible.
    NotHeadlined,
    /// A trivially-prefixed report (a comparison that cannot claim novelty).
    TriviallyPrefixed,
    /// An exploratory report (never the sole basis of acceptance).
    Exploratory,
    /// A guarded report (results bounded by declared caveats).
    Guarded,
}

impl ReportLabelKind {
    /// The canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            ReportLabelKind::Headlined => "headlined",
            ReportLabelKind::NotHeadlined => "not_headlined",
            ReportLabelKind::TriviallyPrefixed => "trivially_prefixed",
            ReportLabelKind::Exploratory => "exploratory",
            ReportLabelKind::Guarded => "guarded",
        }
    }

    /// Parse; `None` on any other input.
    pub fn parse(s: &str) -> Option<ReportLabelKind> {
        match s {
            "headlined" => Some(ReportLabelKind::Headlined),
            "not_headlined" => Some(ReportLabelKind::NotHeadlined),
            "trivially_prefixed" => Some(ReportLabelKind::TriviallyPrefixed),
            "exploratory" => Some(ReportLabelKind::Exploratory),
            "guarded" => Some(ReportLabelKind::Guarded),
            _ => None,
        }
    }
}

/// `ReportLabel{kind}` — the label record.
#[derive(Debug, Clone, PartialEq)]
pub struct ReportLabel {
    /// The label kind.
    pub kind: ReportLabelKind,
}

/// `ComparisonReport` — the amended ADR-0046 record (§6.4 A2; R-2.10.4⁰ᵃ):
/// `{arm_a, arm_b, metric, pairing, paired_effect{point, interval, method},
/// per_task_effects_ref, budget_match{limits_equal, consumption_imbalance,
/// tolerance, status}, benefit_kind, held_out, estimated?, test{kind},
/// sign_profile, tail_effects, outcome_bounds, multiplicity, label,
/// EstimatorSelection per interval}`.
#[derive(Debug, Clone, PartialEq)]
pub struct ComparisonReport {
    /// Arm A's ref.
    pub arm_a: String,
    /// Arm B's ref.
    pub arm_b: String,
    /// The metric compared.
    pub metric: String,
    /// The pairing used (`by_task | by_task_and_replicate`).
    pub pairing: String,
    /// The paired-effect estimate.
    pub paired_effect: PairedEffect,
    /// The per-task effects table ref.
    pub per_task_effects_ref: Option<String>,
    /// The budget-match record.
    pub budget_match: BudgetMatch,
    /// What the comparison measures.
    pub benefit_kind: BenefitKind,
    /// Whether the comparison ran over held-out splits.
    pub held_out: bool,
    /// `estimated{estimator, inclusion_probabilities_ref}` — set on
    /// adaptive-strategy rows (ADR-0156; `search_time_benefit` only).
    pub estimated: Option<Json>,
    /// The test record.
    pub test: TestRecord,
    /// The sign profile.
    pub sign_profile: SignProfile,
    /// The tail effects.
    pub tail_effects: TailEffects,
    /// The outcome bounds.
    pub outcome_bounds: OutcomeBounds,
    /// The multiplicity record.
    pub multiplicity: Multiplicity,
    /// The report label — mandatory: every "X beats Y" claim carries one.
    pub label: ReportLabelKind,
    /// The `EstimatorSelection` for the paired-effect interval (CF-337).
    pub estimator_selection: EstimatorSelection,
}

impl ComparisonReport {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("arm_a".into(), Json::str(&self.arm_a));
        m.insert("arm_b".into(), Json::str(&self.arm_b));
        m.insert("metric".into(), Json::str(&self.metric));
        m.insert("pairing".into(), Json::str(&self.pairing));
        let mut pe = BTreeMap::new();
        if let Some(p) = &self.paired_effect.point {
            pe.insert("point".into(), p.clone());
        }
        if let Some(i) = &self.paired_effect.interval {
            pe.insert("interval".into(), i.clone());
        }
        pe.insert("method".into(), self.paired_effect.method.to_json());
        m.insert("paired_effect".into(), Json::Obj(pe));
        if let Some(r) = &self.per_task_effects_ref {
            m.insert("per_task_effects_ref".into(), Json::str(r));
        }
        let mut bm = BTreeMap::new();
        bm.insert(
            "limits_equal".into(),
            Json::Bool(self.budget_match.limits_equal),
        );
        if let Some(c) = &self.budget_match.consumption_imbalance {
            bm.insert("consumption_imbalance".into(), c.clone());
        }
        bm.insert(
            "tolerance".into(),
            Json::Int(self.budget_match.tolerance_ppm as i64),
        );
        bm.insert("status".into(), Json::str(self.budget_match.status.name()));
        m.insert("budget_match".into(), Json::Obj(bm));
        m.insert("benefit_kind".into(), Json::str(self.benefit_kind.name()));
        m.insert("held_out".into(), Json::Bool(self.held_out));
        if let Some(e) = &self.estimated {
            m.insert("estimated".into(), e.clone());
        }
        m.insert(
            "test".into(),
            Json::obj([("kind", Json::str(self.test.kind.name()))]),
        );
        m.insert(
            "sign_profile".into(),
            Json::obj([
                ("helped", Json::Int(self.sign_profile.helped as i64)),
                ("hurt", Json::Int(self.sign_profile.hurt as i64)),
                ("unchanged", Json::Int(self.sign_profile.unchanged as i64)),
            ]),
        );
        m.insert(
            "tail_effects".into(),
            Json::obj([
                ("P50", self.tail_effects.p50.clone()),
                ("P95", self.tail_effects.p95.clone()),
                ("max", self.tail_effects.max.clone()),
            ]),
        );
        m.insert(
            "outcome_bounds".into(),
            Json::obj([
                ("lower", self.outcome_bounds.lower.clone()),
                ("upper", self.outcome_bounds.upper.clone()),
                ("verdict", Json::str(self.outcome_bounds.verdict.name())),
            ]),
        );
        {
            let mut mult = BTreeMap::new();
            mult.insert(
                "family".into(),
                Json::Int(self.multiplicity.family_size as i64),
            );
            mult.insert("adjusted".into(), Json::str(&self.multiplicity.adjusted));
            if let Some(r) = self.multiplicity.raw_ppm {
                mult.insert("raw".into(), Json::Int(r));
            }
            if let Some(a) = self.multiplicity.adjusted_ppm {
                mult.insert("adjusted_ppm".into(), Json::Int(a));
            }
            if let Some(l) = &self.multiplicity.label {
                mult.insert("label".into(), Json::str(l));
            }
            m.insert("multiplicity".into(), Json::Obj(mult));
        }
        m.insert("label".into(), Json::str(self.label.name()));
        m.insert(
            "estimator_selection".into(),
            self.estimator_selection.to_json(),
        );
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<ComparisonReport, SchemaError> {
        const REC: &str = "ComparisonReport";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "arm_a",
                "arm_b",
                "metric",
                "pairing",
                "paired_effect",
                "per_task_effects_ref",
                "budget_match",
                "benefit_kind",
                "held_out",
                "estimated",
                "test",
                "sign_profile",
                "tail_effects",
                "outcome_bounds",
                "multiplicity",
                "label",
                "estimator_selection",
            ],
            REC,
        )?;
        let pe = expect_obj(member_at(m, "paired_effect", REC)?, "PairedEffect")?;
        reject_unknown(pe, &["point", "interval", "method"], "PairedEffect")?;
        let bm = expect_obj(member_at(m, "budget_match", REC)?, "BudgetMatch")?;
        reject_unknown(
            bm,
            &[
                "limits_equal",
                "consumption_imbalance",
                "tolerance",
                "status",
            ],
            "BudgetMatch",
        )?;
        let test = expect_obj(member_at(m, "test", REC)?, "test")?;
        let sp = expect_obj(member_at(m, "sign_profile", REC)?, "SignProfile")?;
        reject_unknown(sp, &["helped", "hurt", "unchanged"], "SignProfile")?;
        let te = expect_obj(member_at(m, "tail_effects", REC)?, "TailEffects")?;
        reject_unknown(te, &["P50", "P95", "max"], "TailEffects")?;
        let ob = expect_obj(member_at(m, "outcome_bounds", REC)?, "OutcomeBounds")?;
        reject_unknown(ob, &["lower", "upper", "verdict"], "OutcomeBounds")?;
        let mult = expect_obj(member_at(m, "multiplicity", REC)?, "Multiplicity")?;
        reject_unknown(
            mult,
            &["family", "adjusted", "raw", "adjusted_ppm", "label"],
            "Multiplicity",
        )?;
        Ok(ComparisonReport {
            arm_a: str_at(m, "arm_a", REC)?.to_string(),
            arm_b: str_at(m, "arm_b", REC)?.to_string(),
            metric: str_at(m, "metric", REC)?.to_string(),
            pairing: str_at(m, "pairing", REC)?.to_string(),
            paired_effect: PairedEffect {
                point: pe
                    .get("point")
                    .filter(|j| !matches!(j, Json::Null))
                    .cloned(),
                interval: pe
                    .get("interval")
                    .filter(|j| !matches!(j, Json::Null))
                    .cloned(),
                method: IntervalMethod::from_json(member_at(pe, "method", "PairedEffect")?)
                    .ok_or_else(|| SchemaError::v("method", "unknown interval method"))?,
            },
            per_task_effects_ref: opt_str_at(m, "per_task_effects_ref")?.map(str::to_string),
            budget_match: BudgetMatch {
                limits_equal: bool_at(bm, "limits_equal", "BudgetMatch")?,
                consumption_imbalance: bm
                    .get("consumption_imbalance")
                    .filter(|j| !matches!(j, Json::Null))
                    .cloned(),
                tolerance_ppm: int_at(bm, "tolerance", "BudgetMatch")? as u64,
                status: BudgetMatchStatus::parse(str_at(bm, "status", "BudgetMatch")?)
                    .ok_or_else(|| SchemaError::v("status", "unknown budget-match status"))?,
            },
            benefit_kind: BenefitKind::parse(str_at(m, "benefit_kind", REC)?)
                .ok_or_else(|| SchemaError::v("benefit_kind", "unknown benefit kind"))?,
            held_out: bool_at(m, "held_out", REC)?,
            estimated: m
                .get("estimated")
                .filter(|j| !matches!(j, Json::Null))
                .cloned(),
            test: TestRecord {
                kind: TestKind::parse(str_at(test, "kind", "test")?)
                    .ok_or_else(|| SchemaError::v("kind", "unknown test kind"))?,
            },
            sign_profile: SignProfile {
                helped: int_at(sp, "helped", "SignProfile")? as u64,
                hurt: int_at(sp, "hurt", "SignProfile")? as u64,
                unchanged: int_at(sp, "unchanged", "SignProfile")? as u64,
            },
            tail_effects: TailEffects {
                p50: member_at(te, "P50", "TailEffects")?.clone(),
                p95: member_at(te, "P95", "TailEffects")?.clone(),
                max: member_at(te, "max", "TailEffects")?.clone(),
            },
            outcome_bounds: OutcomeBounds {
                lower: member_at(ob, "lower", "OutcomeBounds")?.clone(),
                upper: member_at(ob, "upper", "OutcomeBounds")?.clone(),
                verdict: OutcomeBoundsVerdict::parse(str_at(ob, "verdict", "OutcomeBounds")?)
                    .ok_or_else(|| SchemaError::v("verdict", "unknown bounds verdict"))?,
            },
            multiplicity: Multiplicity {
                family_size: int_at(mult, "family", "Multiplicity")? as u32,
                adjusted: str_at(mult, "adjusted", "Multiplicity")?.to_string(),
                raw_ppm: opt_int_at(mult, "raw")?,
                adjusted_ppm: opt_int_at(mult, "adjusted_ppm")?,
                label: opt_str_at(mult, "label")?.map(str::to_string),
            },
            label: ReportLabelKind::parse(str_at(m, "label", REC)?)
                .ok_or_else(|| SchemaError::v("label", "unknown report label"))?,
            estimator_selection: EstimatorSelection::from_json(member_at(
                m,
                "estimator_selection",
                REC,
            )?)
            .map_err(|e| SchemaError::v("estimator_selection", format!("{e:?}")))?,
        })
    }
}

/// `RenderSpec{panels, source_refs}` — the report's render declaration
/// (§6.4).
#[derive(Debug, Clone, PartialEq)]
pub struct RenderSpec {
    /// The declared panels.
    pub panels: Vec<String>,
    /// The records the panels render.
    pub source_refs: Vec<String>,
}

impl RenderSpec {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "panels",
                Json::Arr(self.panels.iter().map(Json::str).collect()),
            ),
            (
                "source_refs",
                Json::Arr(self.source_refs.iter().map(Json::str).collect()),
            ),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<RenderSpec, SchemaError> {
        const REC: &str = "RenderSpec";
        let m = expect_obj(j, REC)?;
        reject_unknown(m, &["panels", "source_refs"], REC)?;
        Ok(RenderSpec {
            panels: str_vec_at(m, "panels", REC)?,
            source_refs: str_vec_at(m, "source_refs", REC)?,
        })
    }
}

// ── ParityReport ────────────────────────────────────────────────────────────

/// `ParityReport` — the §5h.4 import parity report: a
/// `ComparisonReport{benefit_kind = artifact_benefit}` plus the replay
/// members `{original_runner_ref, original_runs, replay_verdicts}`
/// (ADR-0144).
#[derive(Debug, Clone, PartialEq)]
pub struct ParityReport {
    /// The comparison (`benefit_kind = artifact_benefit` — checked by
    /// `SuiteManifest::validate`).
    pub comparison: ComparisonReport,
    /// The original runner's ref (the suite's original harness).
    pub original_runner_ref: String,
    /// The original run ids replayed.
    pub original_runs: Vec<String>,
    /// The per-run replay verdicts.
    pub replay_verdicts: Vec<String>,
}

impl ParityReport {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("comparison", self.comparison.to_json()),
            ("original_runner_ref", Json::str(&self.original_runner_ref)),
            (
                "original_runs",
                Json::Arr(self.original_runs.iter().map(Json::str).collect()),
            ),
            (
                "replay_verdicts",
                Json::Arr(self.replay_verdicts.iter().map(Json::str).collect()),
            ),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<ParityReport, SchemaError> {
        const REC: &str = "ParityReport";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "comparison",
                "original_runner_ref",
                "original_runs",
                "replay_verdicts",
            ],
            REC,
        )?;
        Ok(ParityReport {
            comparison: ComparisonReport::from_json(member_at(m, "comparison", REC)?)?,
            original_runner_ref: str_at(m, "original_runner_ref", REC)?.to_string(),
            original_runs: str_vec_at(m, "original_runs", REC)?,
            replay_verdicts: str_vec_at(m, "replay_verdicts", REC)?,
        })
    }
}

// ── ScorecardReport (S3.3; spec §5h.2 §2.1 `render_scorecard`; R-2.9.2) ─────
//
// The scorecard is the deterministic, hash-stable render over the results
// plane: one `ConfigurationSummary` per configuration, one `MetricCell` per
// (configuration × metric). Every cell carries the point estimate, the
// interval with its `EstimatorSelection`, the distribution reference, the
// tails (P50/P95/max/catastrophic-failure rate) and the per-task `(c, n)`
// counts so pass^k/pass@k stay recomputable. A cell is `value | n/a{reason}`
// — never 0 by default (ADR-0045 D5/D6). The report names
// `metric_registry_version`, `price_table_version` and the results
// `watermark`; veto-tripped runs are excluded from headline success and
// counted beside it; strata are never pooled unannotated
// (`StrataPooledUnannotated` — the renderer refuses).

/// `MetricCell` — one `(configuration, metric)` scorecard cell.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricCell {
    /// The metric name (a `MetricDeclaration` registry ref).
    pub metric: String,
    /// The point estimate — `MetricValueKind` (`n/a{reason}` typed; never a
    /// coerced 0).
    pub point: hh_ontology::eval::MetricValueKind,
    /// The interval `[lo, hi]` in the metric's unit, when defined.
    pub interval: Option<(i64, i64)>,
    /// The `EstimatorSelection` the interval ran under.
    pub estimator: Option<EstimatorSelection>,
    /// The content address of the distribution rows the cell summarises
    /// (`idp/1` over the canonical per-run values — the distribution is
    /// *referenced*, never inlined-opaquely).
    pub distribution_ref: Option<String>,
    /// `tails{p50, p95, max, catastrophic_rate_ppm}`.
    pub tails: Option<Tails>,
    /// The per-task `(c, n)` counts — pass^k/pass@k stay recomputable.
    pub per_task: Vec<TaskCount>,
    /// Per-outcome-class excluded counts (`outcome_class → n`) — runs outside
    /// the denominator are counted beside, never silently dropped.
    pub excluded: BTreeMap<String, u64>,
    /// The number of runs excluded by tripped vetoes (counted beside the
    /// headline, never merged into it).
    pub vetoed: u64,
    /// The contamination stratum the cell renders (`Some` on every cell —
    /// capability is rendered per stratum and pooling without annotation is
    /// refused; ADR-0143 L5, AC-R-2.9.4-7).
    pub stratum: Option<String>,
}

/// `tails{p50, p95, max, catastrophic_rate_ppm}` — the distributional tail
/// summary every scorecard cell reports (spec §5h.2 AC-8).
#[derive(Debug, Clone, PartialEq)]
pub struct Tails {
    /// The median.
    pub p50: i64,
    /// The 95th percentile.
    pub p95: i64,
    /// The maximum.
    pub max: i64,
    /// The catastrophic-failure rate (ppm) — the share of runs at the metric's
    /// declared catastrophic floor (0 for rate metrics; the declaration's
    /// `direction` picks the tail).
    pub catastrophic_rate_ppm: i64,
}

/// `per_task[{task_id, c, n}]` — the success counts a pass^k/pass@k cell
/// stays recomputable from.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskCount {
    /// The task id.
    pub task_id: String,
    /// Successes.
    pub c: u64,
    /// Trials (denominator-policy runs).
    pub n: u64,
}

/// `ConfigurationSummary` — one scorecard row (one configuration).
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigurationSummary {
    /// The configuration id (`configuration_id` — the seedless coordinate).
    pub configuration_id: String,
    /// The participant class.
    pub participant_class: String,
    /// The runs that fed this summary.
    pub run_ids: Vec<String>,
    /// The per-metric cells.
    pub cells: Vec<MetricCell>,
    /// Vetoed-success count — successes on veto-tripped runs, counted beside
    /// the headline (never inside it).
    pub vetoed_successes: u64,
    /// The portability cell (AC-R-2.9.2-10; CF-417): `bool{true}` only when
    /// the configuration's runs evidence ≥ 2 distinct model snapshots across
    /// ≥ 2 declared families and ≥ 1 held-out level; `n/a{not_run}`
    /// otherwise — a portability claim is never fabricated.
    pub portability: hh_ontology::eval::MetricValueKind,
}

/// `ScorecardReport/1` — the `render_scorecard` output.
#[derive(Debug, Clone, PartialEq)]
pub struct ScorecardReport {
    /// `scorecard_id = H(canonical(minus id))` under `eval.scorecard` — the
    /// hash-stable id (the render is pure over the inputs + watermark).
    pub scorecard_id: String,
    /// The metric-registry version the cells were rendered under.
    pub metric_registry_version: String,
    /// The pricing-table version spend metrics read against (`"none"` when no
    /// spend metric is in the catalogue view).
    pub price_table_version: String,
    /// The results watermark the render reads at.
    pub watermark: u64,
    /// The per-configuration summaries.
    pub configurations: Vec<ConfigurationSummary>,
    /// The strata the render separated (contamination strata never pooled —
    /// L5; the member documents what separated, the renderer refuses an
    /// unannotated pool).
    pub strata: Vec<String>,
    /// The report label (headline eligibility at the report level).
    pub label: ReportLabelKind,
}

fn tails_json(t: &Tails) -> Json {
    Json::obj([
        ("p50", Json::Int(t.p50)),
        ("p95", Json::Int(t.p95)),
        ("max", Json::Int(t.max)),
        ("catastrophic_rate_ppm", Json::Int(t.catastrophic_rate_ppm)),
    ])
}

fn tails_from_json(j: &Json) -> Result<Tails, SchemaError> {
    let m = expect_obj(j, "Tails")?;
    Ok(Tails {
        p50: int_at(m, "p50", "Tails")?,
        p95: int_at(m, "p95", "Tails")?,
        max: int_at(m, "max", "Tails")?,
        catastrophic_rate_ppm: int_at(m, "catastrophic_rate_ppm", "Tails")?,
    })
}

fn task_count_json(t: &TaskCount) -> Json {
    Json::obj([
        ("task_id", Json::str(&t.task_id)),
        ("c", Json::Int(t.c as i64)),
        ("n", Json::Int(t.n as i64)),
    ])
}

fn task_count_from_json(j: &Json) -> Result<TaskCount, SchemaError> {
    let m = expect_obj(j, "TaskCount")?;
    Ok(TaskCount {
        task_id: str_at(m, "task_id", "TaskCount")?.to_string(),
        c: int_at(m, "c", "TaskCount")? as u64,
        n: int_at(m, "n", "TaskCount")? as u64,
    })
}

impl MetricCell {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("metric".into(), Json::str(&self.metric));
        m.insert("point".into(), self.point.to_json());
        if let Some((lo, hi)) = self.interval {
            m.insert(
                "interval".into(),
                Json::obj([("lo", Json::Int(lo)), ("hi", Json::Int(hi))]),
            );
        }
        if let Some(e) = &self.estimator {
            m.insert("estimator".into(), e.to_json());
        }
        if let Some(d) = &self.distribution_ref {
            m.insert("distribution_ref".into(), Json::str(d));
        }
        if let Some(t) = &self.tails {
            m.insert("tails".into(), tails_json(t));
        }
        m.insert(
            "per_task".into(),
            Json::Arr(self.per_task.iter().map(task_count_json).collect()),
        );
        if !self.excluded.is_empty() {
            m.insert(
                "excluded".into(),
                Json::Obj(
                    self.excluded
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::Int(*v as i64)))
                        .collect(),
                ),
            );
        }
        if self.vetoed > 0 {
            m.insert("vetoed".into(), Json::Int(self.vetoed as i64));
        }
        if let Some(st) = &self.stratum {
            m.insert("stratum".into(), Json::str(st));
        }
        Json::Obj(m)
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<MetricCell, SchemaError> {
        const REC: &str = "MetricCell";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "metric",
                "point",
                "interval",
                "estimator",
                "distribution_ref",
                "tails",
                "per_task",
                "excluded",
                "vetoed",
                "stratum",
            ],
            REC,
        )?;
        let interval = match m.get("interval") {
            Some(i) => Some((
                i.get("lo")
                    .and_then(Json::as_int)
                    .ok_or_else(|| SchemaError::v("interval", "missing lo"))?,
                i.get("hi")
                    .and_then(Json::as_int)
                    .ok_or_else(|| SchemaError::v("interval", "missing hi"))?,
            )),
            None => None,
        };
        let mut excluded = BTreeMap::new();
        if let Some(Json::Obj(ex)) = m.get("excluded") {
            for (k, v) in ex {
                excluded.insert(
                    k.clone(),
                    v.as_int()
                        .ok_or_else(|| SchemaError::v("excluded", "count must be int"))?
                        as u64,
                );
            }
        }
        Ok(MetricCell {
            metric: str_at(m, "metric", REC)?.to_string(),
            point: hh_ontology::eval::MetricValueKind::from_json(member_at(m, "point", REC)?)
                .ok_or_else(|| SchemaError::v("point", "unknown value kind"))?,
            interval,
            estimator: m
                .get("estimator")
                .map(EstimatorSelection::from_json)
                .transpose()
                .map_err(|e| SchemaError::v("estimator", format!("{e:?}")))?,
            distribution_ref: opt_str_at(m, "distribution_ref")?.map(str::to_string),
            tails: m.get("tails").map(tails_from_json).transpose()?,
            per_task: arr_at(m, "per_task", REC)?
                .iter()
                .map(task_count_from_json)
                .collect::<Result<Vec<_>, _>>()?,
            excluded,
            vetoed: opt_int_at(m, "vetoed")?.unwrap_or(0) as u64,
            stratum: opt_str_at(m, "stratum")?.map(str::to_string),
        })
    }
}

impl ConfigurationSummary {
    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("configuration_id", Json::str(&self.configuration_id)),
            ("participant_class", Json::str(&self.participant_class)),
            (
                "run_ids",
                Json::Arr(self.run_ids.iter().map(Json::str).collect()),
            ),
            (
                "cells",
                Json::Arr(self.cells.iter().map(|c| c.to_json()).collect()),
            ),
            ("vetoed_successes", Json::Int(self.vetoed_successes as i64)),
            ("portability", self.portability.to_json()),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<ConfigurationSummary, SchemaError> {
        const REC: &str = "ConfigurationSummary";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "configuration_id",
                "participant_class",
                "run_ids",
                "cells",
                "vetoed_successes",
                "portability",
            ],
            REC,
        )?;
        Ok(ConfigurationSummary {
            configuration_id: str_at(m, "configuration_id", REC)?.to_string(),
            participant_class: str_at(m, "participant_class", REC)?.to_string(),
            run_ids: str_vec_at(m, "run_ids", REC)?,
            cells: arr_at(m, "cells", REC)?
                .iter()
                .map(MetricCell::from_json)
                .collect::<Result<Vec<_>, _>>()?,
            vetoed_successes: int_at(m, "vetoed_successes", REC)? as u64,
            portability: hh_ontology::eval::MetricValueKind::from_json(
                m.get("portability")
                    .ok_or_else(|| SchemaError::v("portability", "missing member"))?,
            )
            .ok_or_else(|| SchemaError::v("portability", "unknown form"))?,
        })
    }
}

impl ScorecardReport {
    /// `scorecard_id = H(canonical(minus scorecard_id))` under
    /// `eval.scorecard` — recomputed by the renderer, never trusted.
    pub fn compute_id(&self) -> String {
        let mut j = self.to_json();
        if let Json::Obj(ref mut m) = j {
            m.remove("scorecard_id");
        }
        hh_identity::idp::idp_id("eval.scorecard", j.to_canonical_string().as_bytes())
    }

    /// The canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str("scorecard_report/1")),
            ("scorecard_id", Json::str(&self.scorecard_id)),
            (
                "metric_registry_version",
                Json::str(&self.metric_registry_version),
            ),
            ("price_table_version", Json::str(&self.price_table_version)),
            ("watermark", Json::Int(self.watermark as i64)),
            (
                "configurations",
                Json::Arr(self.configurations.iter().map(|c| c.to_json()).collect()),
            ),
            (
                "strata",
                Json::Arr(self.strata.iter().map(Json::str).collect()),
            ),
            ("label", Json::str(self.label.name())),
        ])
    }

    /// Strict decode.
    pub fn from_json(j: &Json) -> Result<ScorecardReport, SchemaError> {
        const REC: &str = "ScorecardReport/1";
        let m = expect_obj(j, REC)?;
        reject_unknown(
            m,
            &[
                "schema",
                "scorecard_id",
                "metric_registry_version",
                "price_table_version",
                "watermark",
                "configurations",
                "strata",
                "label",
            ],
            REC,
        )?;
        Ok(ScorecardReport {
            scorecard_id: str_at(m, "scorecard_id", REC)?.to_string(),
            metric_registry_version: str_at(m, "metric_registry_version", REC)?.to_string(),
            price_table_version: str_at(m, "price_table_version", REC)?.to_string(),
            watermark: int_at(m, "watermark", REC)? as u64,
            configurations: arr_at(m, "configurations", REC)?
                .iter()
                .map(ConfigurationSummary::from_json)
                .collect::<Result<Vec<_>, _>>()?,
            strata: str_vec_at(m, "strata", REC)?,
            label: ReportLabelKind::parse(str_at(m, "label", REC)?)
                .ok_or_else(|| SchemaError::v("label", "unknown label kind"))?,
        })
    }
}
