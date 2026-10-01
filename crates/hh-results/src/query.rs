//! `QuerySpec` + `Page` — the typed `query_rows` surface (§6.5 §2.2;
//! ADR-0161 D4). Filters are over *declared* fields only — there is no name
//! field (R-ROW-2 kills name-keyed aggregation); `n/a` cells are filterable
//! by reason.

use std::collections::BTreeMap;

use hh_wire::json::Json;

use crate::error::ResultsError;
use crate::row::ResultsRow;
use crate::watermark::WatermarkSet;

/// The declared filterable fields (closed set — `UnknownField` on anything
/// else; §6.5 §2.2 "filters typed over declared fields").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    /// `coordinates.configuration_id`.
    ConfigurationId,
    /// `coordinates.configuration_version_id` (the key's factor member).
    ConfigurationVersionId,
    /// `coordinates.participant_class`.
    ParticipantClass,
    /// `outcome.outcome_class`.
    OutcomeClass,
    /// `coordinates.task.task_id`.
    TaskId,
    /// `coordinates.task.suite_id`.
    SuiteId,
    /// `coordinates.task.split_label`.
    SplitLabel,
    /// `experiment.arm_id`.
    ArmId,
    /// `experiment.cell_id`.
    CellId,
    /// `experiment.experiment_run_id`.
    ExperimentRunId,
    /// `cell[metric].value == n/a{reason}` — the n/a-reason filter.
    CellNaReason {
        /// The metric.
        metric: String,
        /// The `n/a` reason spelling.
        reason: String,
    },
}

impl Field {
    /// Parse the field name (the `cell_na:<metric>` form carries the metric
    /// in the field name; its value is the reason).
    pub fn parse(name: &str) -> Result<Field, ResultsError> {
        Ok(match name {
            "configuration_id" => Field::ConfigurationId,
            "configuration_version_id" => Field::ConfigurationVersionId,
            "participant_class" => Field::ParticipantClass,
            "outcome_class" => Field::OutcomeClass,
            "task_id" => Field::TaskId,
            "suite_id" => Field::SuiteId,
            "split_label" => Field::SplitLabel,
            "arm_id" => Field::ArmId,
            "cell_id" => Field::CellId,
            "experiment_run_id" => Field::ExperimentRunId,
            n if n.starts_with("cell_na:") => Field::CellNaReason {
                metric: n["cell_na:".len()..].to_string(),
                reason: String::new(),
            },
            other => {
                return Err(ResultsError::UnknownField {
                    field: other.to_string(),
                })
            }
        })
    }
}

/// `QuerySpec{filter, projection, at?, page{cursor, limit}, order}` —
/// `filter` is `field → value` conjunctive; `order` ∈ `{"key"}` (the only
/// declared order at Stage 3 — stable under appends by construction).
#[derive(Debug, Clone, Default)]
pub struct QuerySpec {
    /// Conjunctive `field → value` filters.
    pub filters: Vec<(Field, String)>,
    /// The watermark set to serve at (`None` = current heads).
    pub at: Option<WatermarkSet>,
    /// Page cursor (a `key_id` — the next page starts after it).
    pub cursor: Option<String>,
    /// Page limit.
    pub limit: Option<usize>,
}

impl QuerySpec {
    /// The unfiltered query.
    pub fn all() -> QuerySpec {
        QuerySpec::default()
    }

    /// Add a filter (`Field::parse` gates the name — `UnknownField`).
    pub fn filter(mut self, field: &str, value: &str) -> Result<QuerySpec, ResultsError> {
        let mut f = Field::parse(field)?;
        if let Field::CellNaReason { reason, .. } = &mut f {
            *reason = value.to_string();
        }
        self.filters.push((f, value.to_string()));
        Ok(self)
    }

    /// Whether `row` satisfies every filter.
    pub fn matches(&self, row: &ResultsRow) -> Result<bool, ResultsError> {
        for (f, v) in &self.filters {
            let hit = match f {
                Field::ConfigurationId => {
                    row.coordinates.configuration_id.as_deref() == Some(v.as_str())
                }
                Field::ConfigurationVersionId => row.coordinates.configuration_version_id == *v,
                Field::ParticipantClass => row.coordinates.participant_class == *v,
                Field::OutcomeClass => row.outcome.outcome_class.as_deref() == Some(v.as_str()),
                Field::TaskId => {
                    row.coordinates
                        .task
                        .as_ref()
                        .and_then(|t| t.get("task_id"))
                        .and_then(Json::as_str)
                        == Some(v.as_str())
                }
                Field::SuiteId => {
                    row.coordinates
                        .task
                        .as_ref()
                        .and_then(|t| t.get("suite_id"))
                        .and_then(Json::as_str)
                        == Some(v.as_str())
                }
                Field::SplitLabel => {
                    row.coordinates
                        .task
                        .as_ref()
                        .and_then(|t| t.get("split_label"))
                        .and_then(Json::as_str)
                        == Some(v.as_str())
                }
                Field::ArmId => {
                    row.experiment
                        .as_ref()
                        .and_then(|e| e.get("arm_id"))
                        .and_then(Json::as_str)
                        == Some(v.as_str())
                }
                Field::CellId => {
                    row.experiment
                        .as_ref()
                        .and_then(|e| e.get("cell_id"))
                        .and_then(Json::as_str)
                        == Some(v.as_str())
                }
                Field::ExperimentRunId => {
                    row.experiment
                        .as_ref()
                        .and_then(|e| e.get("experiment_run_id"))
                        .and_then(Json::as_str)
                        == Some(v.as_str())
                }
                Field::CellNaReason { metric, reason } => row
                    .cells
                    .iter()
                    .find(|c| &c.metric_ref == metric)
                    .and_then(|c| c.na_reason())
                    .map(|r| r.as_str() == reason)
                    .unwrap_or(false),
            };
            if !hit {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// `Page{rows, next_cursor, watermark_set, view_hash}` — the read result
/// (the served watermark set + view hash ride every page — §6.5 §2.2).
#[derive(Debug, Clone)]
pub struct Page {
    /// The matched head rows (key-ordered).
    pub rows: Vec<ResultsRow>,
    /// The cursor for the next page (`None` = exhausted).
    pub next_cursor: Option<String>,
    /// The watermark set the page was served at.
    pub watermark_set: WatermarkSet,
    /// `idp/1` over `{rows, watermark_set}` — the page's byte identity.
    pub view_hash: String,
}

/// The query row's projection surface — kept for API completeness; at
/// Stage 3 reads return whole rows (the `projection` member is reserved —
/// narrowing lands with the K2/K3 surface consumers, never a name lookup).
pub type Projection = BTreeMap<String, Json>;
