//! `annotate(scope)` — the rebuildable annotation index (§6.5 §2.1;
//! ADR-0161 D3). Annotations are *derived* facts about a row — the covering
//! `bundle_refs[]` from the catalogue, the experiment-plane exclusion/
//! supersession flags — and they are never written into row bytes (CF-298:
//! the row is the immutable projection; the index sits beside it).
//! Deleting the index and re-running `annotate` reproduces it exactly.
//!
//! The Stage-3 sources are the bundle catalogue and the experiment views;
//! `StaleIndex`/snapshot-driven marks are C1 productions (the index schema
//! carries the `flags[]` slot they will fill).

use std::fs;

use hh_identity::idp::idp_id;
use hh_ledger::manifest::RunKind;
use hh_ledger::store::Store;
use hh_wire::json::Json;

use crate::catalogue::BundleStatus;
use crate::error::ResultsError;
use crate::row::ResultsRow;
use crate::store::ResultsStore;
use crate::watermark::WatermarkSet;

/// `RowAnnotations{key, version_id, bundle_refs[], flags[]}` — the
/// rebuildable sidecar for one stored row.
#[derive(Debug, Clone, PartialEq)]
pub struct RowAnnotations {
    /// The fixed key's pinned id.
    pub key: String,
    /// The head version annotated.
    pub version_id: String,
    /// The catalogue bundle ids covering the subject run.
    pub bundle_refs: Vec<String>,
    /// The derived flags — `open` (unfinished), `excluded:<reason>`,
    /// `superseded`, `not_comparable`, `bundle_unvalidated` (a covering
    /// bundle exists below `validated`).
    pub flags: Vec<String>,
}

impl RowAnnotations {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("key", Json::str(&self.key)),
            ("version_id", Json::str(&self.version_id)),
            (
                "bundle_refs",
                Json::Arr(self.bundle_refs.iter().map(Json::str).collect()),
            ),
            (
                "flags",
                Json::Arr(self.flags.iter().map(Json::str).collect()),
            ),
        ])
    }
}

/// `AnnotationIndex{entries[], watermark_set, view_hash}` — the persisted
/// `annotate` product.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnnotationIndex {
    /// Per-row annotations, `key`-ordered.
    pub entries: Vec<RowAnnotations>,
    /// The source prefixes scanned.
    pub watermark_set: WatermarkSet,
    /// `idp/1` over the canonical index.
    pub view_hash: String,
}

impl AnnotationIndex {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str("hh-annotation-index/1")),
            (
                "entries",
                Json::Arr(self.entries.iter().map(|e| e.to_json()).collect()),
            ),
            ("watermark_set", self.watermark_set.to_json()),
            ("view_hash", Json::str(&self.view_hash)),
        ])
    }

    /// The annotations for a key (`None` = unannotated).
    pub fn entry(&self, key_id: &str) -> Option<&RowAnnotations> {
        self.entries.iter().find(|e| e.key == key_id)
    }
}

fn index_path(results: &ResultsStore) -> std::path::PathBuf {
    results.root().join("annotations.json")
}

/// The persisted index (empty when never built).
pub fn load(results: &ResultsStore) -> Result<AnnotationIndex, ResultsError> {
    let path = index_path(results);
    if !path.exists() {
        return Ok(AnnotationIndex::default());
    }
    let text = fs::read_to_string(&path).map_err(|e| ResultsError::Store {
        detail: format!("read annotations: {e}"),
    })?;
    let j = hh_wire::json::parse(&text).map_err(|e| ResultsError::Store {
        detail: format!("parse annotations: {e:?}"),
    })?;
    let Json::Obj(m) = &j else {
        return Err(ResultsError::Store {
            detail: "annotations not an object".into(),
        });
    };
    let mut entries = Vec::new();
    if let Some(Json::Arr(items)) = m.get("entries") {
        for i in items {
            let Json::Obj(e) = i else { continue };
            let strs = |n: &str| -> Vec<String> {
                match e.get(n) {
                    Some(Json::Arr(v)) => v
                        .iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect(),
                    _ => Vec::new(),
                }
            };
            let (Some(k), Some(v)) = (
                e.get("key").and_then(Json::as_str),
                e.get("version_id").and_then(Json::as_str),
            ) else {
                continue;
            };
            entries.push(RowAnnotations {
                key: k.to_string(),
                version_id: v.to_string(),
                bundle_refs: strs("bundle_refs"),
                flags: strs("flags"),
            });
        }
    }
    Ok(AnnotationIndex {
        entries,
        watermark_set: m
            .get("watermark_set")
            .and_then(WatermarkSet::from_json)
            .unwrap_or_default(),
        view_hash: m
            .get("view_hash")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string(),
    })
}

/// The flags a row earns from the experiment plane — every bound
/// experiment's view is folded at head and the attempt's exclusion/
/// supersession/comparable state is lifted.
fn experiment_flags(store: &Store, row: &ResultsRow) -> (Vec<String>, WatermarkSet) {
    let mut flags = Vec::new();
    let mut wm = WatermarkSet::new();
    for run_id in store.run_ids() {
        let Ok(m) = store.manifest(&run_id) else {
            continue;
        };
        if m.run_kind != RunKind::Experiment {
            continue;
        }
        let Ok(head) = store.head(&run_id) else {
            continue;
        };
        let Ok(events) = store.envelopes(&run_id) else {
            continue;
        };
        let view = hh_experiment::view::ExperimentView::fold(events);
        let mut touched = false;
        for plan in view.plans.values() {
            for attempt in &plan.attempts {
                if attempt.run_id != row.key.run_id {
                    continue;
                }
                touched = true;
                if let Some(r) = &attempt.excluded {
                    flags.push(format!("excluded:{r}"));
                }
                if attempt.superseded {
                    flags.push("superseded".to_string());
                }
            }
        }
        if touched {
            wm.pin(&run_id, head.seq);
        }
    }
    if let Some(exp) = &row.experiment {
        if exp.get("comparable").and_then(|c| match c {
            Json::Bool(b) => Some(*b),
            _ => None,
        }) == Some(false)
        {
            flags.push("not_comparable".to_string());
        }
    }
    if row.outcome.status != "finished" {
        flags.push("open".to_string());
    }
    flags.sort();
    flags.dedup();
    (flags, wm)
}

/// `annotate_row(row)` — the per-row sidecar (the `get_row` read composes
/// it fresh — the index is a cache, never the source).
pub fn annotate_row(results: &ResultsStore, store: &Store, row: &ResultsRow) -> RowAnnotations {
    let mut bundle_refs: Vec<String> = results
        .bundle_refs(&row.key.run_id)
        .map(|es| es.iter().map(|e| e.bundle_id.clone()).collect())
        .unwrap_or_default();
    bundle_refs.sort();
    bundle_refs.dedup();
    let (mut flags, _wm) = experiment_flags(store, row);
    // `bundle_unvalidated` — a covering bundle below `validated` keeps the
    // row's L1 admission state visible without a leaderboard call.
    let unvalidated = results
        .bundle_refs(&row.key.run_id)
        .map(|es| {
            !es.is_empty()
                && es
                    .iter()
                    .all(|e| !e.status.satisfies(BundleStatus::Validated))
        })
        .unwrap_or(false);
    if unvalidated {
        flags.push("bundle_unvalidated".to_string());
    }
    // `bundle_superseded` / `bundle_retracted` — the lifecycle terminals
    // annotate the row (AC-R-2.9.3-11: annotated, never removed or
    // hidden; the supersession/retraction edge reads off the status
    // book).
    if let Ok(es) = results.bundle_refs(&row.key.run_id) {
        let any = |want: BundleStatus| es.iter().any(|e| e.status == want);
        if any(BundleStatus::Superseded) {
            flags.push("bundle_superseded".to_string());
        }
        if any(BundleStatus::Retracted) {
            flags.push("bundle_retracted".to_string());
        }
    }
    flags.sort();
    RowAnnotations {
        key: row.key.key_id(),
        version_id: row.version_id.clone(),
        bundle_refs,
        flags,
    }
}

/// `annotate(scope)` — rebuild the index over `scope` (key ids; `None` =
/// every held key), persist `annotations.json`. The watermark set records
/// the experiment runs folded plus the catalogue's own set.
pub fn annotate(
    results: &ResultsStore,
    store: &Store,
    scope: Option<&[String]>,
) -> Result<AnnotationIndex, ResultsError> {
    let keys: Vec<String> = match scope {
        Some(ids) => ids.to_vec(),
        None => results.key_ids(),
    };
    let mut entries = Vec::new();
    let mut wm = WatermarkSet::new();
    for key_id in keys {
        let (row, _) = results.get_row(store, &key_id)?;
        let a = annotate_row(results, store, &row);
        for (r, s) in &row.watermark_set.runs {
            wm.pin(r, *s);
        }
        entries.push(a);
    }
    entries.sort_by(|a, b| a.key.cmp(&b.key));
    if let Ok(cat) = results.catalogue() {
        for (r, s) in &cat.watermark_set.runs {
            wm.pin(r, *s);
        }
    }
    let mut idx = AnnotationIndex {
        entries,
        watermark_set: wm,
        view_hash: String::new(),
    };
    let mut pre = idx.to_json();
    if let Json::Obj(ref mut m) = pre {
        m.remove("view_hash");
    }
    idx.view_hash = idp_id("results_view", pre.to_canonical_string().as_bytes());
    let path = index_path(results);
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, idx.to_json().to_canonical_string()).map_err(|e| ResultsError::Store {
        detail: format!("write annotations: {e}"),
    })?;
    fs::rename(&tmp, &path).map_err(|e| ResultsError::Store {
        detail: format!("rename annotations: {e}"),
    })?;
    Ok(idx)
}
