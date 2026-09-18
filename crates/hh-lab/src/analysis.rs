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

/// `AnalysisSpec` — what the analysis computes (§6.4).
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisSpec {
    /// The spec's id (content-addressed — `analysis_record` domain).
    pub spec_id: String,
    /// The selection.
    pub query: QuerySpec,
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
        m.insert("query".into(), self.query.to_json());
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
                "query",
                "estimator_selection",
                "resample",
                "outputs",
            ],
            REC,
        )?;
        Ok(AnalysisSpec {
            spec_id: str_at(m, "spec_id", REC)?.to_string(),
            query: QuerySpec::from_json(member_at(m, "query", REC)?)?,
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

/// `AnalysisRecord` — the persisted analysis (§6.4; content-addressed under
/// `analysis_record`).
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
        Json::obj([
            ("analysis_id", Json::str(&self.analysis_id)),
            ("spec_ref", Json::str(&self.spec_ref)),
            ("generated_from", self.generated_from.to_json()),
            (
                "outputs",
                Json::Arr(self.outputs.iter().map(Json::str).collect()),
            ),
            ("status", Json::str(self.status.name())),
        ])
    }

    /// Strict decode.
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
        })
    }
}

/// `AnalysisReport` — the rendered analysis output (§6.4).
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

/// `multiplicity{family, adjusted}` — the family-size and correction record
/// (§6.4 A2).
#[derive(Debug, Clone, PartialEq)]
pub struct Multiplicity {
    /// The family size (number of comparisons the correction covers).
    pub family_size: u32,
    /// The correction method (`holm`, `none`, …).
    pub adjusted: String,
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
        m.insert(
            "multiplicity".into(),
            Json::obj([
                ("family", Json::Int(self.multiplicity.family_size as i64)),
                ("adjusted", Json::str(&self.multiplicity.adjusted)),
            ]),
        );
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
        reject_unknown(mult, &["family", "adjusted"], "Multiplicity")?;
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
