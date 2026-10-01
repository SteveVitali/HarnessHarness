//! `verify_row`'s detailed report — the per-check surface behind
//! `ResultsStore::verify_row`'s `ok | Mismatch{recomputed}` verdict (§6.5
//! §2.2/§5.3; AC-R-2.10.5-7): schema round-trip, `version_id`/`view_hash`
//! recomputation, every cited `audit_ref` through `verify_citation` (the
//! head citation plus every cell's `evidence[]`), and full re-projection at
//! the recorded watermark. A failed report is the `AttestationError::Stale`
//! surface — consumers treat a stale row as inadmissible, never as silently
//! absent.

use hh_ledger::store::Store;
use hh_wire::json::Json;

use crate::audit::{self, CitationVerdict};
use crate::error::ResultsError;
use crate::row::ResultsRow;
use crate::store::ResultsStore;

/// One named check in a `VerifyReport`.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifyCheck {
    /// The check name (`schema`, `version_id`, `view_hash`,
    /// `citation:<run>:<seq>`, `reprojection`).
    pub name: String,
    /// Whether the check passed.
    pub ok: bool,
    /// The failure detail (`None` when `ok`).
    pub detail: Option<String>,
}

/// `verify_row`'s report — `{ok, checks[]}`. `ok` is the conjunction;
/// individual failures stay visible (the tamper-evidence contract reports,
/// never hides).
#[derive(Debug, Clone, PartialEq)]
pub struct VerifyReport {
    /// Every check passed.
    pub ok: bool,
    /// The checks, in evaluation order.
    pub checks: Vec<VerifyCheck>,
}

impl VerifyReport {
    fn check(&mut self, name: impl Into<String>, ok: bool, detail: Option<String>) {
        self.checks.push(VerifyCheck {
            name: name.into(),
            ok,
            detail,
        });
        self.ok &= ok;
    }

    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str("hh-verify-report/1")),
            ("ok", Json::Bool(self.ok)),
            (
                "checks",
                Json::Arr(
                    self.checks
                        .iter()
                        .map(|c| {
                            Json::obj([
                                ("name", Json::str(&c.name)),
                                ("ok", Json::Bool(c.ok)),
                                ("detail", c.detail.as_ref().map_or(Json::Null, Json::str)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    }
}

/// The error a consumer sees for a failed report — §6.5's
/// `AttestationError::Stale`.
pub fn stale(selector: &str, report: &VerifyReport) -> ResultsError {
    ResultsError::AttestationStale {
        key: selector.to_string(),
        detail: report
            .checks
            .iter()
            .find(|c| !c.ok)
            .map(|c| format!("{}: {}", c.name, c.detail.clone().unwrap_or_default()))
            .unwrap_or_else(|| "verification failed".into()),
    }
}

/// `verify_row(version_id)` — load the stored version and run the check
/// set. `UnknownVersion` when the store does not hold it.
pub fn verify_row(
    results: &ResultsStore,
    store: &Store,
    docs: Option<&hh_experiment::docs::LabDocs>,
    version_id: &str,
) -> Result<VerifyReport, ResultsError> {
    let row = results.get_version(version_id)?;
    let mut report = verify_row_value(store, &row);
    // Re-projection — the deterministic-repeat arm. Re-project at the
    // recorded watermark and compare `version_id` (the `Mismatch` the §2.2
    // verdict reports).
    let rerun = results.project_row(
        store,
        docs,
        &row.derived_from.run_id,
        Some(row.derived_from.seq),
        Some(&row.scoring),
    );
    match rerun {
        Ok(r) => report.check(
            "reprojection",
            r.version_id == row.version_id,
            (r.version_id != row.version_id)
                .then(|| format!("re-projection yields {} — row is stale", r.version_id)),
        ),
        Err(e) => report.check(
            "reprojection",
            false,
            Some(format!("re-projection refused: {e}")),
        ),
    }
    Ok(report)
}

/// The check set over an in-hand row (no re-projection — `verify_row`
/// adds that arm; a caller holding only the row still gets schema,
/// identity, and every citation).
pub fn verify_row_value(store: &Store, row: &ResultsRow) -> VerifyReport {
    let mut report = VerifyReport {
        ok: true,
        checks: Vec::new(),
    };
    // 1. Schema — the row must round-trip through `ResultsRow/1`.
    let schema_ok = ResultsRow::from_json(&row.to_json())
        .map(|r| r == *row)
        .unwrap_or(false);
    report.check(
        "schema",
        schema_ok,
        (!schema_ok).then(|| "ResultsRow/1 round-trip failed".to_string()),
    );
    // 2. `version_id` — the content address over the row preimage.
    let want_v = row.compute_version_id();
    report.check(
        "version_id",
        want_v == row.version_id,
        (want_v != row.version_id)
            .then(|| format!("recomputed {want_v}, row carries {}", row.version_id)),
    );
    // 3. `view_hash` — the watermark-set binding.
    let want_h = row.compute_view_hash();
    report.check(
        "view_hash",
        want_h == row.view_hash,
        (want_h != row.view_hash)
            .then(|| format!("recomputed {want_h}, row carries {}", row.view_hash)),
    );
    // 4. Every citation — the head plus every cell's `evidence[]` — through
    // `verify_citation` (hash match + Merkle inclusion + content_refs).
    let mut cites: Vec<&crate::audit::AuditRef> = vec![&row.audit.head];
    for c in &row.cells {
        cites.extend(c.evidence.iter());
    }
    for a in cites {
        let name = format!("citation:{}:{}", a.run_id, a.seq);
        match audit::verify_citation(store, a) {
            CitationVerdict::Verified => report.check(name, true, None),
            v => report.check(name, false, Some(v.as_str())),
        }
    }
    report
}
