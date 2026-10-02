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
use crate::version::DerivedReason;
use crate::watermark::WatermarkSet;

/// `RowAnnotations{key, version_id, bundle_refs[], flags[], leaders[],
/// scoring_marks[]}` — the rebuildable sidecar for one stored row
/// (C1: the `leaders`/`scoring_marks` members land — §6.5 §2.1).
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
    /// bundle exists below `validated`), `contaminated` (a
    /// `security.containment.violated` row on the subject run),
    /// `member_revoked`/`suite_retired`/`evidence_missing` (the C1
    /// snapshot-overlay marks; annotate, never hide).
    pub flags: Vec<String>,
    /// The retained leaderboard snapshots admitting this key —
    /// `leaderboards/snapshots/*` membership, derived (never recorded on
    /// the row — CF-298).
    pub leaders: Vec<String>,
    /// The score-derivation marks — `rescored:<reason>` per non-initial
    /// version and `analysis:<event_ref>` per `measurement.analysis.
    /// recorded` row citing this version.
    pub scoring_marks: Vec<String>,
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
            (
                "leaders",
                Json::Arr(self.leaders.iter().map(Json::str).collect()),
            ),
            (
                "scoring_marks",
                Json::Arr(self.scoring_marks.iter().map(Json::str).collect()),
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
                leaders: strs("leaders"),
                scoring_marks: strs("scoring_marks"),
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
            // `suite_retired` — a `measurement.suite.retired` row on the
            // bound experiment run marks every row of the retired suite
            // (AC-R-2.10.5-4's third flag; the retirement is a ledger
            // fact, the flag a derived overlay — annotate, never hide).
            // When the row names a `suite_ref`/`suite_id`, a row whose
            // task suite differs is not marked.
            let row_suite = row
                .coordinates
                .task
                .as_ref()
                .and_then(|t| t.get("suite_id"))
                .and_then(Json::as_str);
            let retired = events.iter().any(|e| {
                if e.class != "measurement.suite.retired" {
                    return false;
                }
                let named = e
                    .payload
                    .get("suite_ref")
                    .or_else(|| e.payload.get("suite_id"))
                    .or_else(|| e.payload.get("suite"))
                    .and_then(Json::as_str);
                match (named, row_suite) {
                    (Some(n), Some(s)) => n == s,
                    _ => true,
                }
            });
            if retired {
                flags.push("suite_retired".to_string());
            }
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
    // `contaminated` — a `security.containment.violated` row on the
    // subject run marks the row (the leaderboard L8 veto consumes this
    // flag; annotate, never hide).
    if let Ok(events) = store.envelopes(&row.key.run_id) {
        if events
            .iter()
            .any(|e| e.class == "security.containment.violated")
        {
            flags.push("contaminated".to_string());
        }
    }
    if row.outcome.status != "finished" {
        flags.push("open".to_string());
    }
    flags.sort();
    flags.dedup();
    (flags, wm)
}

/// The score-derivation marks — `rescored:<reason>` for every non-initial
/// version on the key's chain and `analysis:<event_id>` for each
/// `measurement.analysis.recorded` row citing the row's `version_id` or
/// key (the §6.5 §2.1 `scoring_marks[]` member).
fn scoring_marks(results: &ResultsStore, store: &Store, row: &ResultsRow) -> Vec<String> {
    let mut marks = Vec::new();
    if let Ok(history) = results.row_history(&row.key.key_id()) {
        for v in &history {
            if v.derived_from_reason != DerivedReason::Initial {
                marks.push(format!("rescored:{}", v.derived_from_reason.as_str()));
            }
        }
    }
    // Analysis rows cite the version — a `measurement.analysis.recorded`
    // payload carrying the version id (or the key id) marks the row.
    let needles = [row.version_id.clone(), row.key.key_id()];
    'runs: for run_id in store.run_ids() {
        let Ok(events) = store.envelopes(&run_id) else {
            continue;
        };
        for e in events {
            if e.class != "measurement.analysis.recorded" {
                continue;
            }
            let s = e.payload.to_canonical_string();
            if needles.iter().any(|n| s.contains(n.as_str())) {
                marks.push(format!("analysis:{}", e.event_id));
                continue 'runs;
            }
        }
    }
    marks.sort();
    marks.dedup();
    marks
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
    // `member_revoked`/`suite_retired` — a retracted bundle reads as a
    // revoked member; a suite the row's cells name that is marked
    // retired in the registry/annotations source flags `suite_retired`.
    // (The flags spell out the §6.5 §4 snapshot-overlay vocabulary so the
    // leaderboard diff reports them as flag changes, never removals.)
    if flags.iter().any(|f| f == "bundle_retracted") {
        flags.push("member_revoked".to_string());
    }
    flags.sort();
    flags.dedup();
    let leaders = crate::leaderboard::snapshot_memberships(results, &row.key.key_id());
    let marks = scoring_marks(results, store, row);
    RowAnnotations {
        key: row.key.key_id(),
        version_id: row.version_id.clone(),
        bundle_refs,
        flags,
        leaders,
        scoring_marks: marks,
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
