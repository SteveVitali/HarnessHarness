//! `export_rows(query, target, policy)` — the results export surface
//! (§6.5 §2.2; ADR-0161 D4; ADR-0294 D3):
//!
//! - `ledger_native_rows` — the row set **verbatim** (canonical
//!   `ResultsRow` JSON); lossless by construction (`LoweringLossReport`
//!   is empty; the rows decode back to equal `version_id`s —
//!   AC-R-2.10.5-11's round-trip).
//! - `interchange_table` / `csv_like_table` / `foreign_leaderboard_submission`
//!   — a lowered form: one row per `configuration_id` × metric cell,
//!   `{configuration_id, metric_ref, value}`. Every member that has no
//!   slot in the target is *listed* in the `LoweringLossReport` —
//!   `n/a{reason}` cells, intervals, provenance, evidence refs, the
//!   audit head, consumption detail — `no_slot`, never lowered to `0`
//!   or empty (T-LCD-11/-15).
//!
//! The artefact is content-addressed and written under
//! `<root>/exports/<address>.json`; `measurement.export.delivered` rides
//! `Store::commit_kernel_row_for` onto every cited run (the one ledger
//! append an exporter may make — §5h.1 §6).

use std::fs;

use hh_identity::idp::idp_id;
use hh_ledger::store::Store;
use hh_wire::json::Json;

use crate::error::ResultsError;
use crate::row::ResultsRow;
use crate::store::ResultsStore;

/// The export targets (§6.5 §2.2 `target ∈ {…}` — a closed vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportTarget {
    /// `ledger_native_rows` — lossless canonical rows.
    LedgerNativeRows,
    /// `interchange_table` — the typed interchange lowering.
    InterchangeTable,
    /// `csv_like_table` — the flat table lowering.
    CsvLikeTable,
    /// `foreign_leaderboard_submission` — the foreign leaderboard form.
    ForeignLeaderboardSubmission,
}

impl ExportTarget {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ExportTarget::LedgerNativeRows => "ledger_native_rows",
            ExportTarget::InterchangeTable => "interchange_table",
            ExportTarget::CsvLikeTable => "csv_like_table",
            ExportTarget::ForeignLeaderboardSubmission => "foreign_leaderboard_submission",
        }
    }

    /// Parse; `None` on any other spelling.
    pub fn parse(s: &str) -> Option<ExportTarget> {
        match s {
            "ledger_native_rows" => Some(ExportTarget::LedgerNativeRows),
            "interchange_table" => Some(ExportTarget::InterchangeTable),
            "csv_like_table" => Some(ExportTarget::CsvLikeTable),
            "foreign_leaderboard_submission" => Some(ExportTarget::ForeignLeaderboardSubmission),
            _ => None,
        }
    }

    /// Whether the target preserves the row bytes verbatim.
    pub fn lossless(self) -> bool {
        matches!(self, ExportTarget::LedgerNativeRows)
    }
}

/// One loss-report entry — `{run_id, field, reason, detail}` where
/// `reason ∈ {no_slot}` at this tier (the member exists on the canonical
/// row but the target carries no slot for it).
#[derive(Debug, Clone, PartialEq)]
pub struct LossEntry {
    /// The subject run.
    pub run_id: String,
    /// The member that did not lower.
    pub field: String,
    /// The loss reason (`no_slot` — the target has no place for it).
    pub reason: String,
    /// The carried detail (the `n/a` reason, the member's shape).
    pub detail: String,
}

impl LossEntry {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("run_id", Json::str(&self.run_id)),
            ("field", Json::str(&self.field)),
            ("reason", Json::str(&self.reason)),
            ("detail", Json::str(&self.detail)),
        ])
    }
}

/// `LoweringLossReport{target, entries[], lossless}` — every dropped or
/// typed-slot member listed; empty on `ledger_native_rows`.
#[derive(Debug, Clone, PartialEq)]
pub struct LoweringLossReport {
    /// The export target.
    pub target: String,
    /// Whether the export is lossless (only `ledger_native_rows`).
    pub lossless: bool,
    /// The entries.
    pub entries: Vec<LossEntry>,
}

impl LoweringLossReport {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str("hh-results-loss-report/1")),
            ("target", Json::str(&self.target)),
            ("lossless", Json::Bool(self.lossless)),
            (
                "entries",
                Json::Arr(self.entries.iter().map(|e| e.to_json()).collect()),
            ),
        ])
    }

    /// The report's content address (the `loss_report_ref` the
    /// `measurement.export.delivered` rows carry).
    pub fn content_ref(&self) -> hh_identity::idp::ContentAddress {
        hh_identity::idp::address(
            self.to_json().to_canonical_string().as_bytes(),
            "application/vnd.hh.loss-report+json",
        )
    }
}

/// The export outcome — `(artefact, loss_report, delivered[])` per §6.5.
#[derive(Debug, Clone)]
pub struct ExportOutcome {
    /// The artefact's content address (the bytes live under
    /// `<results>/exports/<address>.json`).
    pub artefact: String,
    /// The lowering loss report.
    pub loss_report: LoweringLossReport,
    /// The runs `measurement.export.delivered` was committed on.
    pub delivered: Vec<String>,
    /// The artefact's canonical JSON (the lowered form / the verbatim set).
    pub body: Json,
}

fn io_err(detail: impl Into<String>) -> ResultsError {
    ResultsError::Store {
        detail: detail.into(),
    }
}

/// The lowered foreign-table form of one cell — the value lowers only
/// when concrete; `n/a{reason}` is a loss entry, never a `0`.
fn lower_row(row: &ResultsRow, loss: &mut Vec<LossEntry>) -> Vec<Json> {
    let mut out = Vec::new();
    for cell in &row.cells {
        match &cell.value {
            hh_ontology::eval::MetricValueKind::Na(reason) => {
                loss.push(LossEntry {
                    run_id: row.key.run_id.clone(),
                    field: format!("cells.{}.value", cell.metric_ref),
                    reason: "no_slot".into(),
                    detail: format!("n/a{{{}}}", reason.as_str()),
                });
            }
            v => out.push(Json::obj([
                (
                    "configuration_id",
                    Json::str(row.coordinates.configuration_id.clone().unwrap_or_default()),
                ),
                ("run_id", Json::str(&row.key.run_id)),
                ("metric_ref", Json::str(&cell.metric_ref)),
                ("value", v.to_json()),
            ])),
        }
        if let Some(r) = &cell.oracle_ref {
            loss.push(LossEntry {
                run_id: row.key.run_id.clone(),
                field: format!("cells.{}.oracle_ref", cell.metric_ref),
                reason: "no_slot".into(),
                detail: format!("provenance member not lowerable: {r}"),
            });
        }
    }
    // The members a foreign leaderboard submission carries no slot for —
    // every one is listed, never silently dropped (T-LCD-11).
    loss.push(LossEntry {
        run_id: row.key.run_id.clone(),
        field: "audit.head".into(),
        reason: "no_slot".into(),
        detail: "the row's audit_ref has no foreign slot".into(),
    });
    if let Some(spend) = row.consumption.spend.as_ref() {
        loss.push(LossEntry {
            run_id: row.key.run_id.clone(),
            field: "consumption.spend".into(),
            reason: "no_slot".into(),
            detail: format!(
                "cost provenance/confidence/coverage members have no foreign slot (provenance {})",
                spend
                    .get("provenance")
                    .and_then(|p| p.get("kind"))
                    .and_then(Json::as_str)
                    .unwrap_or("?")
            ),
        });
    }
    out
}

/// `export_rows(rows, target, policy)` — `rows` are the resolved
/// `ResultsRow`s the caller selected (`query_rows`/`get_row` at the
/// boundary); the artefact + loss report are derived, written under
/// `exports/`, and `measurement.export.delivered` lands on every cited
/// run through `commit_kernel_row_for`.
pub fn export_rows(
    results: &ResultsStore,
    store: &mut Store,
    rows: &[ResultsRow],
    target: ExportTarget,
    policy: &Json,
) -> Result<ExportOutcome, ResultsError> {
    let mut entries: Vec<LossEntry> = Vec::new();
    let body = if target.lossless() {
        Json::obj([
            ("schema", Json::str("hh-results-export/1")),
            ("target", Json::str(target.as_str())),
            (
                "rows",
                Json::Arr(rows.iter().map(|r| r.to_json()).collect()),
            ),
            ("policy", policy.clone()),
        ])
    } else {
        let mut lowered = Vec::new();
        for r in rows {
            lowered.extend(lower_row(r, &mut entries));
        }
        Json::obj([
            ("schema", Json::str("hh-results-export/1")),
            ("target", Json::str(target.as_str())),
            ("rows", Json::Arr(lowered)),
            ("policy", policy.clone()),
        ])
    };
    let report = LoweringLossReport {
        target: target.as_str().to_string(),
        lossless: entries.is_empty() && target.lossless(),
        entries,
    };
    // Embed the report into the artefact so the bytes are self-describing.
    let body = if let Json::Obj(mut m) = body {
        m.insert("loss_report".into(), report.to_json());
        Json::Obj(m)
    } else {
        body
    };
    let artefact = idp_id("results_export", body.to_canonical_string().as_bytes());
    let dir = results.root().join("exports");
    fs::create_dir_all(&dir).map_err(|e| io_err(format!("create exports: {e}")))?;
    let path = dir.join(format!("{}.json", artefact.replace(':', "_")));
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, body.to_canonical_string()).map_err(|e| io_err(format!("write: {e}")))?;
    fs::rename(&tmp, &path).map_err(|e| io_err(format!("rename: {e}")))?;

    // `measurement.export.delivered` on every cited run — kernel-row
    // append, durable-before-visible (§5h.1 §6; the only append an
    // exporter may make).
    let mut delivered = Vec::new();
    let loss_ref = report.content_ref();
    let mut cited: Vec<String> = rows.iter().map(|r| r.key.run_id.clone()).collect();
    cited.sort();
    cited.dedup();
    for run_id in &cited {
        let head = store.head(run_id).map_err(ResultsError::from)?;
        let payload = Json::obj([
            ("sink_id", Json::str(&artefact)),
            ("view_kind", Json::str(target.as_str())),
            (
                "seq_range",
                Json::Arr(vec![Json::Int(0), Json::Int(head.seq as i64)]),
            ),
            ("content_classes", Json::Arr(vec![Json::str("results_row")])),
            ("loss_report_ref", Json::str(loss_ref.id())),
        ]);
        store
            .commit_kernel_row_for(
                "hh-results/1",
                run_id,
                "measurement.export.delivered",
                payload,
                vec![loss_ref.clone()],
                vec![],
            )
            .map_err(ResultsError::from)?;
        delivered.push(run_id.clone());
    }
    Ok(ExportOutcome {
        artefact,
        loss_report: report,
        delivered,
        body,
    })
}

/// `ledger_native_rows` round-trip — decode the artefact's `rows[]` back
/// into `ResultsRow`s (equal `version_id`s prove the lossless claim —
/// AC-R-2.10.5-11).
pub fn round_trip_native(body: &Json) -> Result<Vec<String>, ResultsError> {
    let mut ids = Vec::new();
    if let Some(Json::Arr(rows)) = body.get("rows") {
        for r in rows {
            ids.push(ResultsRow::from_json(r)?.version_id);
        }
    }
    Ok(ids)
}
