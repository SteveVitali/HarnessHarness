//! `status(bundle_id)` / `set_status(bundle_id, status, reason,
//! authority) → BundleStatusRecord` — the lifecycle-by-record surface
//! (§5h.3 §2/§3; ADR-0141 D3; S4.2; AC-R-2.9.3-7/-11/-14).
//!
//! Rules the contract fixes:
//! - `status ∈ {assembled, validated, attested, published, restricted,
//!   reproduced, superseded, retracted}`; transitions monotone on the
//!   evidence ladder except `published ↔ restricted` and `any →
//!   retracted`; a terminal (`superseded`/`retracted`) never leaves.
//! - `validated` requires a `valid` validation report; `attested` an
//!   attestation on the manifest; `reproduced` a
//!   `ReproReport{verdict = reproduced, independent = true}` (AC-7 —
//!   producer-signed reproduction is repeatability, never the basis);
//!   `published`/`restricted` require a `valid` report (AC-14: a bundle
//!   carrying secret material is invalid and unpublishable);
//!   `superseded` requires the successor's `derived_from` edge.
//! - `retracted` never deletes — result rows annotate
//!   `bundle_retracted`; the record is append-only (`history[]` keeps
//!   every transition).
//! - Every transition appends `measurement.experiment
//!   .bundle_status_changed` to a subject run — the book folds from the
//!   ledger, so the persisted `bundle_status.json` is a rebuildable
//!   cache (CC3/CC4).

use std::collections::BTreeMap;
use std::fs;

use hh_ledger::store::Store;
use hh_wire::json::{parse as json_parse, Json};

use crate::catalogue::BundleStatus;
use crate::error::ResultsError;
use crate::store::ResultsStore;

/// The ledger event a transition appends (§5h.3 §3 "status transitions
/// as ledger-visible events where a run is the subject").
pub const STATUS_EVENT: &str = "measurement.experiment.bundle_status_changed";

/// `BundleStatusRecord{bundle_ref, status, readers, reason, supersedes?,
/// provenance, created_at}` (§5h.3 §3) + the transition's `evidence`
/// (the report/attestation/repro-report refs the gate checked) and
/// `from` (the prior status — the record is the transition).
#[derive(Debug, Clone, PartialEq)]
pub struct BundleStatusRecord {
    /// The bundle the record governs.
    pub bundle_ref: String,
    /// The prior status.
    pub from: BundleStatus,
    /// The new status.
    pub status: BundleStatus,
    /// The reader set the status ships under (publication states).
    pub readers: Vec<String>,
    /// The declared reason.
    pub reason: String,
    /// The supersession counterpart — the successor bundle on
    /// `superseded`, the predecessor edge on a successor's own record.
    pub supersedes: Option<String>,
    /// The evidence the gate checked (`{validation_report?, attestation?,
    /// repro_report{verdict, independent}?, successor?}`).
    pub evidence: Json,
    /// The acting authority (the participant/instrument that set it).
    pub provenance: Json,
    /// RFC 3339 ms.
    pub created_at: String,
}

impl BundleStatusRecord {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        m.insert("schema".into(), Json::str("hh-bundle-status/1"));
        m.insert("bundle_ref".into(), Json::str(&self.bundle_ref));
        m.insert("from".into(), Json::str(self.from.as_str()));
        m.insert("status".into(), Json::str(self.status.as_str()));
        m.insert(
            "readers".into(),
            Json::Arr(self.readers.iter().map(Json::str).collect()),
        );
        m.insert("reason".into(), Json::str(&self.reason));
        if let Some(s) = &self.supersedes {
            m.insert("supersedes".into(), Json::str(s));
        }
        m.insert("evidence".into(), self.evidence.clone());
        m.insert("provenance".into(), self.provenance.clone());
        m.insert("created_at".into(), Json::str(&self.created_at));
        Json::Obj(m)
    }

    /// Decode.
    pub fn from_json(j: &Json) -> Option<BundleStatusRecord> {
        Some(BundleStatusRecord {
            bundle_ref: j.get("bundle_ref")?.as_str()?.to_string(),
            from: BundleStatus::parse(j.get("from")?.as_str()?)?,
            status: BundleStatus::parse(j.get("status")?.as_str()?)?,
            readers: match j.get("readers") {
                Some(Json::Arr(a)) => a
                    .iter()
                    .filter_map(|r| r.as_str().map(String::from))
                    .collect(),
                _ => vec![],
            },
            reason: j.get("reason")?.as_str()?.to_string(),
            supersedes: j.get("supersedes").and_then(Json::as_str).map(String::from),
            evidence: j.get("evidence").cloned().unwrap_or(Json::Null),
            provenance: j.get("provenance").cloned().unwrap_or(Json::Null),
            created_at: j.get("created_at")?.as_str()?.to_string(),
        })
    }
}

/// The append-only status book — `bundle_ref → records[]` (each record
/// immutable; the fold is chronological).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StatusBook {
    /// `bundle_ref → records[]` oldest-first.
    pub records: BTreeMap<String, Vec<BundleStatusRecord>>,
}

impl StatusBook {
    /// The current status of `bundle_ref` (`assembled` when the book
    /// holds no record).
    pub fn status(&self, bundle_ref: &str) -> BundleStatus {
        self.records
            .get(bundle_ref)
            .and_then(|rs| rs.last())
            .map(|r| r.status)
            .unwrap_or(BundleStatus::Assembled)
    }

    /// The record chain for `bundle_ref` — the `lineage` read's status
    /// leg.
    pub fn history(&self, bundle_ref: &str) -> &[BundleStatusRecord] {
        self.records
            .get(bundle_ref)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str("hh-bundle-status-book/1")),
            (
                "records",
                Json::Obj(
                    self.records
                        .iter()
                        .map(|(k, rs)| {
                            (
                                k.clone(),
                                Json::Arr(rs.iter().map(|r| r.to_json()).collect()),
                            )
                        })
                        .collect(),
                ),
            ),
        ])
    }

    /// Decode.
    pub fn from_json(j: &Json) -> Option<StatusBook> {
        let mut records = BTreeMap::new();
        if let Some(Json::Obj(m)) = j.get("records") {
            for (k, v) in m {
                if let Json::Arr(rs) = v {
                    let recs: Option<Vec<BundleStatusRecord>> =
                        rs.iter().map(BundleStatusRecord::from_json).collect();
                    records.insert(k.clone(), recs?);
                }
            }
        }
        Some(StatusBook { records })
    }
}

fn book_path(results: &ResultsStore) -> std::path::PathBuf {
    results.root().join("bundle_status.json")
}

/// The persisted book (empty when never written — `fold` is the
/// authoritative rebuild over the ledger).
pub fn load_book(results: &ResultsStore) -> Result<StatusBook, ResultsError> {
    let path = book_path(results);
    if !path.exists() {
        return Ok(StatusBook::default());
    }
    let text = fs::read_to_string(&path).map_err(|e| ResultsError::Store {
        detail: format!("read bundle_status: {e}"),
    })?;
    let j = json_parse(&text).map_err(|e| ResultsError::Store {
        detail: format!("parse bundle_status: {e:?}"),
    })?;
    StatusBook::from_json(&j).ok_or_else(|| ResultsError::Store {
        detail: "bundle_status decode failed".into(),
    })
}

fn write_book(results: &ResultsStore, book: &StatusBook) -> Result<(), ResultsError> {
    let path = book_path(results);
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, book.to_json().to_canonical_string()).map_err(|e| ResultsError::Store {
        detail: format!("write bundle_status: {e}"),
    })?;
    fs::rename(&tmp, &path).map_err(|e| ResultsError::Store {
        detail: format!("rename bundle_status: {e}"),
    })?;
    Ok(())
}

/// `fold` — rebuild the book from `STATUS_EVENT` rows over every run
/// (the rebuildable-index rule: the persisted book is a cache of this
/// fold).
pub fn fold(store: &Store) -> StatusBook {
    let mut book = StatusBook::default();
    for run_id in store.run_ids() {
        let Ok(events) = store.envelopes(&run_id) else {
            continue;
        };
        for e in events {
            if e.class != STATUS_EVENT {
                continue;
            }
            let Some(bundle) = e.payload.get("bundle_id").and_then(Json::as_str) else {
                continue;
            };
            let rec = BundleStatusRecord {
                bundle_ref: bundle.to_string(),
                from: e
                    .payload
                    .get("from")
                    .and_then(Json::as_str)
                    .and_then(BundleStatus::parse)
                    .unwrap_or(BundleStatus::Assembled),
                status: e
                    .payload
                    .get("to")
                    .and_then(Json::as_str)
                    .and_then(BundleStatus::parse)
                    .unwrap_or(BundleStatus::Assembled),
                readers: match e.payload.get("readers") {
                    Some(Json::Arr(a)) => a
                        .iter()
                        .filter_map(|r| r.as_str().map(String::from))
                        .collect(),
                    _ => vec![],
                },
                reason: e
                    .payload
                    .get("reason")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string(),
                supersedes: e
                    .payload
                    .get("supersedes")
                    .and_then(Json::as_str)
                    .map(String::from),
                evidence: e.payload.get("evidence").cloned().unwrap_or(Json::Null),
                provenance: e.payload.get("authority").cloned().unwrap_or(Json::Null),
                created_at: e.ts.clone(),
            };
            book.records
                .entry(bundle.to_string())
                .or_default()
                .push(rec);
        }
    }
    book
}

/// `set_status` — the gated transition (§5h.3 §2). `evidence` is the
/// caller-supplied gate proof: `{validation_report?: {status}, attestation?,
/// repro_report?: {bundle_id, verdict, independent}, successor?}` — the
/// op refuses `StatusGateFailed` (a typed refusal, never a skipped
/// check). `repro_report.bundle_id` must equal the transition's
/// `bundle_ref` (S4.4 — evidence binds to the record it attests).
#[allow(clippy::too_many_arguments)] // the status event's fields are the record's shape.
pub fn set_status(
    store: &mut Store,
    results: &ResultsStore,
    subject_run: &str,
    bundle_ref: &str,
    to: BundleStatus,
    reason: &str,
    readers: Vec<String>,
    authority: Json,
    evidence: Json,
) -> Result<BundleStatusRecord, ResultsError> {
    // The current status — the persisted book wins (it caches the event
    // fold); a refresh-minted `validated` is the book's `assembled`
    // floor, never a recorded promotion.
    let mut book = load_book(results)?;
    let from = book.status(bundle_ref);
    if !from.may_transition_to(to) {
        return Err(ResultsError::StatusGateFailed {
            detail: format!(
                "StatusGateFailed: {} → {} is not a legal transition",
                from.as_str(),
                to.as_str()
            ),
        });
    }
    if to == BundleStatus::Retracted && reason.is_empty() {
        return Err(ResultsError::StatusGateFailed {
            detail: "StatusGateFailed: retracted requires a reason".into(),
        });
    }
    let evidence_at = |key: &str| evidence.get(key);
    match to {
        BundleStatus::Validated | BundleStatus::Published | BundleStatus::Restricted => {
            let ok = evidence_at("validation_report")
                .and_then(|r| r.get("status"))
                .and_then(Json::as_str)
                == Some("valid");
            if !ok {
                return Err(ResultsError::StatusGateFailed {
                    detail: format!(
                        "StatusGateFailed: {} requires a `valid` validation report",
                        to.as_str()
                    ),
                });
            }
        }
        BundleStatus::Attested => {
            if evidence_at("attestation").is_none() {
                return Err(ResultsError::StatusGateFailed {
                    detail: "StatusGateFailed: attested requires an attestation".into(),
                });
            }
        }
        BundleStatus::Reproduced => {
            let repro = evidence_at("repro_report");
            let independent = matches!(
                repro.and_then(|r| r.get("independent")),
                Some(Json::Bool(true))
            );
            let verdict = repro.and_then(|r| r.get("verdict")).and_then(Json::as_str);
            // The report must name *this* bundle — a `ReproReport` is
            // evidence for its `bundle_id`, never a transferable badge
            // (S4.4; CC2 — evidence binds to the record it attests).
            let bound = repro
                .and_then(|r| r.get("bundle_id"))
                .and_then(Json::as_str)
                == Some(bundle_ref);
            if !(independent && verdict == Some("reproduced") && bound) {
                return Err(ResultsError::StatusGateFailed {
                    detail: "StatusGateFailed: reproduced requires \
                             ReproReport{verdict = reproduced, independent = true, \
                             bundle_id = this bundle}"
                        .into(),
                });
            }
        }
        BundleStatus::Superseded => {
            let succ = evidence_at("successor").and_then(Json::as_str);
            match succ {
                Some(s) if !s.is_empty() => {}
                _ => {
                    return Err(ResultsError::StatusGateFailed {
                        detail: "StatusGateFailed: superseded requires evidence.successor".into(),
                    })
                }
            }
        }
        _ => {}
    }
    let record = BundleStatusRecord {
        bundle_ref: bundle_ref.to_string(),
        from,
        status: to,
        readers,
        reason: reason.to_string(),
        supersedes: evidence_at("successor")
            .and_then(Json::as_str)
            .map(String::from),
        evidence,
        provenance: authority.clone(),
        created_at: hh_ledger::store::rfc3339_ms(store.now_ms()),
    };
    // The ledger-visible event on the subject run (§5h.3 §3) — the book
    // fold reads it back; the persisted book stays a cache.
    let payload = Json::obj([
        ("bundle_id", Json::str(bundle_ref)),
        ("from", Json::str(from.as_str())),
        ("to", Json::str(to.as_str())),
        ("reason", Json::str(reason)),
        ("authority", authority),
        ("evidence", record.evidence.clone()),
        (
            "readers",
            Json::Arr(record.readers.iter().map(Json::str).collect()),
        ),
        (
            "supersedes",
            record
                .supersedes
                .clone()
                .map(Json::str)
                .unwrap_or(Json::Null),
        ),
    ]);
    store
        .commit_kernel_row_for(
            "kernel:bundle_status",
            subject_run,
            STATUS_EVENT,
            payload,
            vec![],
            vec![],
        )
        .map_err(|e| ResultsError::Store {
            detail: format!("append bundle_status_changed: {e}"),
        })?;
    book.records
        .entry(bundle_ref.to_string())
        .or_default()
        .push(record.clone());
    write_book(results, &book)?;
    Ok(record)
}
