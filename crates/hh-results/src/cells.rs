//! `build_cells(design_ref | experiment_run_id, watermark_set?)` →
//! `CellTable`, and `distribution(configuration_id, metric_ref, split, at?)`
//! → `DistributionRef` (§6.5 §2.1/§2.2; ADR-0161 D3/D4). The cell table is
//! the §6.4 input: excluded rows are present with typed reasons — "latest
//! attempt wins" is never inferred (§6.5 §5).

use std::collections::BTreeMap;

use hh_experiment::docs::LabDocs;
use hh_experiment::view::ExperimentView;
use hh_ledger::manifest::RunKind;
use hh_ledger::store::Store;
use hh_wire::json::Json;

use crate::error::ResultsError;
use crate::row::ResultsRow;
use crate::scoring::ScoringContext;
use crate::store::ResultsStore;
use crate::watermark::WatermarkSet;

/// One `cells[]` member of the `CellTable` — `cell_id = (arm_id,
/// configuration_id, task_id)` (§6.5 §3).
#[derive(Debug, Clone, PartialEq)]
pub struct TableCell {
    /// The plan cell id.
    pub cell_id: String,
    /// The arm.
    pub arm_id: String,
    /// The seedless configuration coordinate.
    pub configuration_id: String,
    /// The task.
    pub task_id: String,
    /// The task's split label.
    pub split_label: String,
    /// The planned-but-ineligible reason, when the plan carries one.
    pub na_reason: Option<String>,
    /// The bound rows — excluded/superseded attempts included with reasons.
    pub rows: Vec<CellRowRef>,
    /// `per_metric{metric_ref → {c, n, values[], outcome_counts,
    /// veto_count}}` over included rows.
    pub per_metric: BTreeMap<String, MetricAggregate>,
}

/// `rows[]{key, version_id, included, excluded_reason?}` inside a cell.
#[derive(Debug, Clone, PartialEq)]
pub struct CellRowRef {
    /// The fixed key's pinned id.
    pub key: String,
    /// The row version observed.
    pub version_id: String,
    /// Whether the row contributes to `per_metric` (not excluded, not
    /// superseded, finished).
    pub included: bool,
    /// The typed exclusion reason when not included.
    pub excluded_reason: Option<String>,
}

/// `per_metric{c, n, values[], outcome_counts, veto_count}` — `n` = rows
/// with a concrete value, `c` = affirmative count (`bool true`/`verdict P`);
/// `values[]` carries the full distribution (sorted — canonical).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MetricAggregate {
    /// Affirmative-value count.
    pub c: u64,
    /// Concrete-value count.
    pub n: u64,
    /// The observed values (canonical `MetricValueKind` JSON, sorted by
    /// canonical string for byte-repeatability).
    pub values: Vec<Json>,
    /// `outcome_class → count` over included rows.
    pub outcome_counts: BTreeMap<String, u64>,
    /// Included rows carrying ≥1 veto trip.
    pub veto_count: u64,
}

/// `CellTable{design_ref, watermark_set, cells[], view_hash}` — the §6.4
/// input (§6.5 §3).
#[derive(Debug, Clone, PartialEq)]
pub struct CellTable {
    /// The experiment's content address (the design the plan expanded).
    pub design_ref: String,
    /// The experiment run this table was folded from.
    pub experiment_run_id: String,
    /// The exact source prefixes read.
    pub watermark_set: WatermarkSet,
    /// The cells.
    pub cells: Vec<TableCell>,
    /// `idp/1` over the canonical table — the rebuild-equality anchor.
    pub view_hash: String,
}

impl CellTable {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str("hh-cell-table/1")),
            ("design_ref", Json::str(&self.design_ref)),
            ("experiment_run_id", Json::str(&self.experiment_run_id)),
            ("watermark_set", self.watermark_set.to_json()),
            (
                "cells",
                Json::Arr(self.cells.iter().map(cell_json).collect()),
            ),
            ("view_hash", Json::str(&self.view_hash)),
        ])
    }
}

fn cell_json(c: &TableCell) -> Json {
    Json::obj([
        ("cell_id", Json::str(&c.cell_id)),
        ("arm_id", Json::str(&c.arm_id)),
        ("configuration_id", Json::str(&c.configuration_id)),
        ("task_id", Json::str(&c.task_id)),
        ("split_label", Json::str(&c.split_label)),
        (
            "na_reason",
            c.na_reason.as_ref().map_or(Json::Null, Json::str),
        ),
        (
            "rows",
            Json::Arr(
                c.rows
                    .iter()
                    .map(|r| {
                        Json::obj([
                            ("key", Json::str(&r.key)),
                            ("version_id", Json::str(&r.version_id)),
                            ("included", Json::Bool(r.included)),
                            (
                                "excluded_reason",
                                r.excluded_reason.as_ref().map_or(Json::Null, Json::str),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "per_metric",
            Json::Obj(
                c.per_metric
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            Json::obj([
                                ("c", Json::Int(v.c as i64)),
                                ("n", Json::Int(v.n as i64)),
                                ("values", Json::Arr(v.values.clone())),
                                (
                                    "outcome_counts",
                                    Json::Obj(
                                        v.outcome_counts
                                            .iter()
                                            .map(|(o, n)| (o.clone(), Json::Int(*n as i64)))
                                            .collect(),
                                    ),
                                ),
                                ("veto_count", Json::Int(v.veto_count as i64)),
                            ]),
                        )
                    })
                    .collect(),
            ),
        ),
    ])
}

/// Resolve the target to `(experiment_run_id)`: a held `run_kind =
/// experiment` run id, else a `design_ref` resolved through `LabDocs`'s
/// experiment index.
fn resolve_experiment(store: &Store, docs: &LabDocs, target: &str) -> Result<String, ResultsError> {
    if let Ok(m) = store.manifest(target) {
        if m.run_kind == RunKind::Experiment {
            return Ok(target.to_string());
        }
        return Err(ResultsError::NotSubjectRun {
            run_id: target.to_string(),
            run_kind: m.run_kind.as_str().to_string(),
        });
    }
    if let Ok(Some(entry)) = docs.index_entry(target) {
        if let Some(run_id) = entry.run_id {
            return Ok(run_id);
        }
    }
    Err(ResultsError::DesignNotFound {
        reference: target.to_string(),
    })
}

/// `build_cells(design_ref | experiment_run_id, watermark_set?)` — pure
/// fold over the experiment run's view + the bound subject prefixes.
/// Refuses `MissingMatchSpec` when the declared kind requires a match and
/// an arm lacks `MatchSpec`; `at` caps every source prefix (a source ahead
/// of `at` is `WatermarkAhead`).
pub fn build_cells(
    results: &ResultsStore,
    store: &Store,
    docs: &LabDocs,
    target: &str,
    at: Option<&WatermarkSet>,
) -> Result<CellTable, ResultsError> {
    Ok(build_cells_inner(results, store, docs, target, at)?.0)
}

/// `build_cells` plus the freshly projected rows it referenced — the
/// projections are *not* recorded (a derived table never writes), so
/// consumers that need row bodies (the leaderboard) take them here rather
/// than resolving `version_id`s the store may never have recorded.
pub(crate) fn build_cells_inner(
    results: &ResultsStore,
    store: &Store,
    docs: &LabDocs,
    target: &str,
    at: Option<&WatermarkSet>,
) -> Result<(CellTable, BTreeMap<String, ResultsRow>), ResultsError> {
    let experiment_run_id = resolve_experiment(store, docs, target)?;
    let exp_head = store.head(&experiment_run_id).map_err(ResultsError::from)?;
    let exp_seq = match at.and_then(|a| a.get(&experiment_run_id)) {
        Some(s) if s > exp_head.seq => {
            return Err(ResultsError::WatermarkAhead {
                run_id: experiment_run_id.clone(),
                seq: s,
                head: Some(exp_head.seq),
            });
        }
        Some(s) => s,
        None => exp_head.seq,
    };
    let events: Vec<hh_ledger::event::EventEnvelope> = store
        .envelopes(&experiment_run_id)
        .map_err(ResultsError::from)?
        .iter()
        .filter(|e| e.seq <= exp_seq)
        .cloned()
        .collect();
    let view = ExperimentView::fold(&events);
    let declared = view
        .declared
        .clone()
        .ok_or_else(|| ResultsError::DesignNotFound {
            reference: format!("{experiment_run_id}: no declared record"),
        })?;
    let spec = docs
        .spec(&declared.experiment_id)
        .map_err(|e| ResultsError::Store {
            detail: format!("labdocs spec: {e:?}"),
        })?
        .ok_or_else(|| ResultsError::DesignNotFound {
            reference: declared.experiment_id.clone(),
        })?;
    let plan = docs
        .plan(&declared.plan_id)
        .map_err(|e| ResultsError::Store {
            detail: format!("labdocs plan: {e:?}"),
        })?
        .ok_or_else(|| ResultsError::DesignNotFound {
            reference: declared.plan_id.clone(),
        })?;

    // `MissingMatchSpec` — a matched-kind design with an un-matched arm
    // refuses the cell table (the E-1 refusal surfaces here; §6.5 §2.1).
    if spec.kind.requires_match() {
        for arm in &spec.arms {
            if arm.match_spec.is_none() {
                return Err(ResultsError::MissingMatchSpec {
                    arm: arm.arm_id.clone(),
                });
            }
        }
    }

    let mut watermark_set = WatermarkSet::new();
    watermark_set.pin(&experiment_run_id, exp_seq);
    let scoring = ScoringContext::native();

    let mut cells: Vec<TableCell> = Vec::new();
    let mut projected: BTreeMap<String, ResultsRow> = BTreeMap::new();
    for cell in &plan.cells {
        let mut rows: Vec<CellRowRef> = Vec::new();
        let mut per_metric: BTreeMap<String, MetricAggregate> = BTreeMap::new();
        for ps in view.plans.values().filter(|p| p.cell_id == cell.cell_id) {
            for attempt in &ps.attempts {
                // Read the subject at its `at` pin (a run absent from `at`
                // reads at head; the produced watermark_set records exactly
                // what was read). A pin beyond the durable head is
                // `WatermarkAhead`.
                let seq = at.and_then(|a| a.get(&attempt.run_id));
                if let Some(s) = seq {
                    let h = store.head(&attempt.run_id).map_err(ResultsError::from)?;
                    if s > h.seq {
                        return Err(ResultsError::WatermarkAhead {
                            run_id: attempt.run_id.clone(),
                            seq: s,
                            head: Some(h.seq),
                        });
                    }
                }
                let row: ResultsRow =
                    results.project_row(store, Some(docs), &attempt.run_id, seq, Some(&scoring))?;
                watermark_set.pin(&attempt.run_id, row.derived_from.seq);
                projected.insert(row.version_id.clone(), row.clone());
                let excluded_reason = if let Some(r) = &attempt.excluded {
                    Some(r.clone())
                } else if attempt.superseded {
                    Some("superseded".to_string())
                } else {
                    None
                };
                let included = excluded_reason.is_none() && row.outcome.status == "finished";
                rows.push(CellRowRef {
                    key: row.key.key_id(),
                    version_id: row.version_id.clone(),
                    included,
                    excluded_reason,
                });
                if included {
                    let oc = row
                        .outcome
                        .outcome_class
                        .clone()
                        .unwrap_or_else(|| "open".to_string());
                    let vetoed = !row.outcome.veto_tripped.is_empty();
                    for c in &row.cells {
                        let agg = per_metric.entry(c.metric_ref.clone()).or_default();
                        *agg.outcome_counts.entry(oc.clone()).or_insert(0) += 1;
                        if vetoed {
                            agg.veto_count += 1;
                        }
                        if c.is_concrete() {
                            agg.n += 1;
                            if affirmative(&c.value) {
                                agg.c += 1;
                            }
                            agg.values.push(c.value.to_json());
                        }
                    }
                }
            }
        }
        for agg in per_metric.values_mut() {
            agg.values.sort_by_key(|a| a.to_canonical_string());
        }
        cells.push(TableCell {
            cell_id: cell.cell_id.clone(),
            arm_id: cell.arm_id.clone(),
            configuration_id: cell.configuration_id.clone(),
            task_id: cell.task_id.clone(),
            split_label: cell.split_label.name().to_string(),
            na_reason: cell.na_reason.map(|r| r.as_str().to_string()),
            rows,
            per_metric,
        });
    }

    let mut table = CellTable {
        design_ref: declared.experiment_id.clone(),
        experiment_run_id,
        watermark_set,
        cells,
        view_hash: String::new(),
    };
    let mut pre = table.to_json();
    if let Json::Obj(ref mut m) = pre {
        m.remove("view_hash");
    }
    table.view_hash =
        hh_identity::idp::idp_id("results_view", pre.to_canonical_string().as_bytes());
    Ok((table, projected))
}

/// The affirmative-value rule for `c` (bool `true` / verdict `P`; decimal
/// and vector values have no affirmative reading — they live in `values[]`).
fn affirmative(v: &hh_ontology::eval::MetricValueKind) -> bool {
    use hh_ontology::eval::MetricValueKind as K;
    match v {
        K::Bool(b) => *b,
        K::Verdict(l) => l.as_str() == "P",
        _ => false,
    }
}

/// `DistributionRef{configuration_id, metric_ref, split, n, values[],
/// quantiles, per_task_cn, watermark_set, view_hash}` — the §6.5 §2.2
/// distribution read over head rows.
#[derive(Debug, Clone, PartialEq)]
pub struct DistributionRef {
    /// The configuration coordinate.
    pub configuration_id: String,
    /// The metric.
    pub metric_ref: String,
    /// The split label filter (`None` = all splits).
    pub split: Option<String>,
    /// Concrete-value count.
    pub n: u64,
    /// The concrete values (canonical-sorted).
    pub values: Vec<Json>,
    /// `{p50, p90, p99}` nearest-rank quantiles over `decimal` values
    /// (absent when no decimal values exist).
    pub quantiles: Option<Json>,
    /// `task_id → {c, n}` — coverage per task (`c` = concrete values,
    /// `n` = rows in the cell).
    pub per_task_cn: BTreeMap<String, (u64, u64)>,
    /// The served watermark set.
    pub watermark_set: WatermarkSet,
    /// The view hash.
    pub view_hash: String,
}

/// `distribution(configuration_id, metric_ref, split, at?)`.
pub fn distribution(
    results: &ResultsStore,
    store: &Store,
    configuration_id: &str,
    metric_ref: &str,
    split: Option<&str>,
    at: Option<&WatermarkSet>,
) -> Result<DistributionRef, ResultsError> {
    let mut spec = crate::query::QuerySpec::all();
    spec = spec.filter("configuration_id", configuration_id)?;
    if let Some(s) = split {
        spec = spec.filter("split_label", s)?;
    }
    spec.at = at.cloned();
    let page = results.query_rows(store, &spec)?;
    let mut values: Vec<Json> = Vec::new();
    let mut decimals: Vec<i64> = Vec::new();
    let mut per_task_cn: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for row in &page.rows {
        let task = row
            .coordinates
            .task
            .as_ref()
            .and_then(|t| t.get("task_id"))
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let e = per_task_cn.entry(task).or_insert((0, 0));
        e.1 += 1;
        if let Some(cell) = row.cells.iter().find(|c| c.metric_ref == metric_ref) {
            if cell.is_concrete() {
                e.0 += 1;
                values.push(cell.value.to_json());
                if let hh_ontology::eval::MetricValueKind::Decimal(d) = cell.value {
                    decimals.push(d);
                }
            }
        }
    }
    values.sort_by_key(|a| a.to_canonical_string());
    decimals.sort_unstable();
    let quantiles = if decimals.is_empty() {
        None
    } else {
        let q = |p: usize| {
            let idx = (p * decimals.len().saturating_sub(1)) / 100;
            Json::Int(decimals[idx])
        };
        Some(Json::obj([("p50", q(50)), ("p90", q(90)), ("p99", q(99))]))
    };
    let mut d = DistributionRef {
        configuration_id: configuration_id.to_string(),
        metric_ref: metric_ref.to_string(),
        split: split.map(String::from),
        n: values.len() as u64,
        values,
        quantiles,
        per_task_cn,
        watermark_set: page.watermark_set.clone(),
        view_hash: String::new(),
    };
    let j = Json::obj([
        ("configuration_id", Json::str(&d.configuration_id)),
        ("metric_ref", Json::str(&d.metric_ref)),
        ("split", d.split.as_ref().map_or(Json::Null, Json::str)),
        ("n", Json::Int(d.n as i64)),
        ("values", Json::Arr(d.values.clone())),
        ("quantiles", d.quantiles.clone().unwrap_or(Json::Null)),
        (
            "per_task_cn",
            Json::Obj(
                d.per_task_cn
                    .iter()
                    .map(|(k, (c, n))| {
                        (
                            k.clone(),
                            Json::obj([("c", Json::Int(*c as i64)), ("n", Json::Int(*n as i64))]),
                        )
                    })
                    .collect(),
            ),
        ),
        ("watermark_set", d.watermark_set.to_json()),
    ]);
    d.view_hash = hh_identity::idp::idp_id("results_view", j.to_canonical_string().as_bytes());
    Ok(d)
}
