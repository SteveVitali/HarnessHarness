//! `ResultsStore` — the derived-state store (§6.5 §1: "everything it holds …
//! is project()-pure at a recorded watermark set, has a view_hash, and
//! rebuilds to equal bytes; it has no update or delete operation"). On disk:
//!
//! ```text
//! <root>/rows/<key_id>/<version_id>.json   — canonical ResultsRow bytes
//! <root>/heads/<key_id>.json               — {key, head, versions[] root-first}
//! <root>/catalogue.json                    — the last catalogue_refresh
//! ```
//!
//! Every file is a rebuildable projection: delete `<root>` and
//! `rebuild_all` reproduces byte-identical contents at the same watermark
//! set (AC-R-2.10.5-1). Writes are write-then-rename, read-only after
//! commit — no mutation op exists.

use std::fs;
use std::path::{Path, PathBuf};

use hh_experiment::docs::LabDocs;
use hh_ledger::store::Store;
use hh_wire::json::{parse as json_parse, Json};

use crate::annotations::{self, RowAnnotations};
use crate::error::ResultsError;
use crate::query::{Page, QuerySpec};
use crate::row::ResultsRow;
use crate::scoring::ScoringContext;
use crate::version::{DerivedReason, RowVersion};
use crate::watermark::WatermarkSet;

fn io_err(detail: impl Into<String>) -> ResultsError {
    ResultsError::Store {
        detail: detail.into(),
    }
}

fn file_name(id: &str) -> String {
    id.replace(':', "_") + ".json"
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ResultsError> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).map_err(|e| io_err(format!("create dir: {e}")))?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|e| io_err(format!("write: {e}")))?;
    fs::rename(&tmp, path).map_err(|e| io_err(format!("rename: {e}")))?;
    Ok(())
}

fn read_json(path: &Path) -> Result<Json, ResultsError> {
    let bytes = fs::read(path).map_err(|e| io_err(format!("read {}: {e}", path.display())))?;
    let text = String::from_utf8(bytes).map_err(|e| io_err(format!("utf8: {e}")))?;
    json_parse(&text).map_err(|e| io_err(format!("parse {}: {e:?}", path.display())))
}

/// The head file — `{key, head, versions[]}` root-first (the version chain
/// is the durable record; the head file is itself rebuildable from the row
/// files' `supersedes` links).
#[derive(Debug, Clone)]
struct HeadFile {
    head: String,
    versions: Vec<RowVersion>,
}

/// `verify_row`'s verdict — `ok | Mismatch{recomputed}` (§6.5 §2.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyVerdict {
    /// The stored version re-projects to the identical `version_id`.
    Ok,
    /// Re-projection produced a different identity — the stored bytes do
    /// not match their declared derivation.
    Mismatch {
        /// What re-projection produced.
        recomputed: String,
    },
}

/// The results store — see module docs.
pub struct ResultsStore {
    root: PathBuf,
    /// The change-journal hub — `record`/`annotate`/`publish` emit a
    /// journal event *after* their durable write returns
    /// (durable-before-visible, §6.5 §2.2 `subscribe`).
    journal: crate::journal::JournalHub,
}

impl ResultsStore {
    /// Open (creating) the derived-state directory at `root`.
    pub fn open(root: impl AsRef<Path>) -> Result<ResultsStore, ResultsError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("rows")).map_err(|e| io_err(format!("create rows: {e}")))?;
        fs::create_dir_all(root.join("heads")).map_err(|e| io_err(format!("create heads: {e}")))?;
        Ok(ResultsStore {
            root,
            journal: crate::journal::JournalHub::default(),
        })
    }

    /// `subscribe(filter)` — the change journal (§6.5 §2.2): a stream of
    /// `RowHeadChanged | AnnotationChanged | SnapshotPublished` events
    /// emitted only after the underlying write is durable.
    pub fn subscribe(
        &self,
        filter: crate::journal::JournalFilter,
    ) -> crate::journal::JournalSubscription {
        self.journal.subscribe(filter)
    }

    /// Emit a journal event post-commit (crate-internal).
    pub(crate) fn journal_emit(&self, e: crate::journal::JournalEvent) {
        self.journal.emit(e);
    }

    /// The store directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    // ── projection + recording ─────────────────────────────────────────

    /// `project_row(run_id, until_seq?, scoring?)` — pure; records nothing.
    pub fn project_row(
        &self,
        store: &Store,
        docs: Option<&LabDocs>,
        run_id: &str,
        until_seq: Option<u64>,
        scoring: Option<&ScoringContext>,
    ) -> Result<ResultsRow, ResultsError> {
        crate::projection::project_row(store, docs, run_id, until_seq, scoring)
    }

    /// Record a projected row as a version under its fixed key. Idempotent
    /// on `version_id` (re-recording the identical projection returns the
    /// existing version); a new version supersedes the current head —
    /// `head(key)` moves, the old bytes are untouched (ADR-0161 D2).
    pub fn record(
        &self,
        row: &ResultsRow,
        reason: DerivedReason,
    ) -> Result<RowVersion, ResultsError> {
        let key_id = row.key.key_id();
        let dir = self.root.join("rows").join(&key_id);
        let head_path = self.root.join("heads").join(format!("{key_id}.json"));
        let prior: Option<HeadFile> = if head_path.exists() {
            Some(self.read_head(&head_path)?)
        } else {
            None
        };
        if let Some(h) = &prior {
            if let Some(v) = h.versions.iter().find(|v| v.version_id == row.version_id) {
                return Ok(v.clone());
            }
        }
        let version = RowVersion {
            key: row.key.to_json(),
            version_id: row.version_id.clone(),
            scoring: row.scoring.clone(),
            supersedes: prior.as_ref().map(|h| h.head.clone()),
            derived_from_reason: reason,
            watermark_set: row.watermark_set.clone(),
        };
        write_atomic(
            &dir.join(file_name(&row.version_id)),
            &row.to_canonical_bytes(),
        )?;
        let mut versions = prior.map(|h| h.versions).unwrap_or_default();
        versions.push(version.clone());
        let head_json = Json::obj([
            ("key", row.key.to_json()),
            ("key_id", Json::str(&key_id)),
            ("head", Json::str(&row.version_id)),
            (
                "versions",
                Json::Arr(versions.iter().map(|v| v.to_json()).collect()),
            ),
        ]);
        write_atomic(&head_path, head_json.to_canonical_string().as_bytes())?;
        // Journal: the head move is durable — emit `row_head_changed`
        // (post-commit emission is the subscribe contract).
        self.journal_emit(crate::journal::JournalEvent {
            kind: crate::journal::JournalKind::RowHeadChanged,
            subject: key_id,
            detail: row.version_id.clone(),
        });
        Ok(version)
    }

    /// `project_row` + `record` in one call — the ordinary "project this run
    /// into the store" path.
    pub fn project_and_record(
        &self,
        store: &Store,
        docs: Option<&LabDocs>,
        run_id: &str,
        until_seq: Option<u64>,
        scoring: Option<&ScoringContext>,
        reason: DerivedReason,
    ) -> Result<(ResultsRow, RowVersion), ResultsError> {
        let row = self.project_row(store, docs, run_id, until_seq, scoring)?;
        let v = self.record(&row, reason)?;
        Ok((row, v))
    }

    /// `rescore(key, scoring, reason)` — re-project the head's run under the
    /// new scoring context and record the version (ADR-0161 D2). Refuses
    /// `NoChange` when the re-projection is byte-identical to the head.
    pub fn rescore(
        &self,
        store: &Store,
        docs: Option<&LabDocs>,
        key_id: &str,
        scoring: &ScoringContext,
        reason: DerivedReason,
    ) -> Result<RowVersion, ResultsError> {
        let head = self.head(key_id)?;
        let head_row = self.load_row(key_id, &head.version_id)?;
        let row = self.project_row(
            store,
            docs,
            &head_row.key.run_id,
            Some(head_row.derived_from.seq),
            Some(scoring),
        )?;
        if row.version_id == head.version_id {
            return Err(ResultsError::NoChange {
                key: key_id.to_string(),
            });
        }
        self.record(&row, reason)
    }

    fn read_head(&self, path: &Path) -> Result<HeadFile, ResultsError> {
        let j = read_json(path)?;
        let Json::Obj(m) = &j else {
            return Err(io_err("head file not an object"));
        };
        let head = m
            .get("head")
            .and_then(Json::as_str)
            .ok_or_else(|| io_err("head file: no head"))?
            .to_string();
        let mut versions = Vec::new();
        if let Some(Json::Arr(items)) = m.get("versions") {
            for i in items {
                versions.push(RowVersion::from_json(i)?);
            }
        }
        Ok(HeadFile { head, versions })
    }

    fn head_file(&self, key_id: &str) -> Result<HeadFile, ResultsError> {
        let path = self.root.join("heads").join(format!("{key_id}.json"));
        if !path.exists() {
            return Err(ResultsError::UnknownKey {
                key: key_id.to_string(),
            });
        }
        self.read_head(&path)
    }

    /// `head(key)` — the current head version record.
    pub fn head(&self, key_id: &str) -> Result<RowVersion, ResultsError> {
        let h = self.head_file(key_id)?;
        h.versions
            .iter()
            .find(|v| v.version_id == h.head)
            .cloned()
            .ok_or(ResultsError::UnknownVersion { version_id: h.head })
    }

    fn load_row(&self, key_id: &str, version_id: &str) -> Result<ResultsRow, ResultsError> {
        let path = self
            .root
            .join("rows")
            .join(key_id)
            .join(file_name(version_id));
        if !path.exists() {
            return Err(ResultsError::UnknownVersion {
                version_id: version_id.to_string(),
            });
        }
        ResultsRow::from_json(&read_json(&path)?)
    }

    /// `get_row(key | version_id)` → the row plus its rebuildable
    /// annotations (§6.5 §2.2 — annotations are never in row bytes, CF-298).
    /// A bare `version_id` resolves across all keys (versions are globally
    /// unique by construction).
    pub fn get_row(
        &self,
        store: &Store,
        selector: &str,
    ) -> Result<(ResultsRow, RowAnnotations), ResultsError> {
        let row = match self.head_file(selector) {
            Ok(h) => self.load_row(selector, &h.head)?,
            Err(ResultsError::UnknownKey { .. }) => self.find_version(selector)?,
            Err(e) => return Err(e),
        };
        let annotations = annotations::annotate_row(self, store, &row);
        Ok((row, annotations))
    }

    /// The row bytes for an exact version.
    pub fn get_version(&self, version_id: &str) -> Result<ResultsRow, ResultsError> {
        self.find_version(version_id)
    }

    fn find_version(&self, version_id: &str) -> Result<ResultsRow, ResultsError> {
        let rows_dir = self.root.join("rows");
        if let Ok(entries) = fs::read_dir(&rows_dir) {
            for e in entries.flatten() {
                if !e.path().is_dir() {
                    continue;
                }
                let key_id = e.file_name().to_string_lossy().to_string();
                let p = e.path().join(file_name(version_id));
                if p.exists() {
                    return self.load_row(&key_id, version_id);
                }
            }
        }
        Err(ResultsError::UnknownVersion {
            version_id: version_id.to_string(),
        })
    }

    /// `row_history(key)` → `[RowVersion]` root-first (§6.5 §2.2).
    pub fn row_history(&self, key_id: &str) -> Result<Vec<RowVersion>, ResultsError> {
        Ok(self.head_file(key_id)?.versions)
    }

    /// Every held key id (rebuild + enumeration; derived from the `heads/`
    /// directory — never authoritative).
    pub fn key_ids(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Ok(entries) = fs::read_dir(self.root.join("heads")) {
            for e in entries.flatten() {
                let n = e.file_name().to_string_lossy().to_string();
                if let Some(k) = n.strip_suffix(".json") {
                    out.push(k.to_string());
                }
            }
        }
        out.sort();
        out
    }

    // ── reads ──────────────────────────────────────────────────────────

    /// `query_rows(QuerySpec)` — typed filters over declared fields, stable
    /// key-ordered pagination, `at` pins the served watermark set (§6.5
    /// §2.2). A row is visible at `at` iff its `watermark_set` is covered —
    /// a later projection never appears inside an earlier watermark's page
    /// (AC-R-2.10.5-5).
    pub fn query_rows(&self, store: &Store, spec: &QuerySpec) -> Result<Page, ResultsError> {
        if let Some(at) = &spec.at {
            for (run_id, seq) in &at.runs {
                match store.head(run_id) {
                    Ok(h) if h.seq >= *seq => {}
                    Ok(h) => {
                        return Err(ResultsError::WatermarkAhead {
                            run_id: run_id.clone(),
                            seq: *seq,
                            head: Some(h.seq),
                        });
                    }
                    Err(_) => {
                        return Err(ResultsError::WatermarkAhead {
                            run_id: run_id.clone(),
                            seq: *seq,
                            head: None,
                        });
                    }
                }
            }
        }
        let mut rows: Vec<ResultsRow> = Vec::new();
        for key_id in self.key_ids() {
            let h = self.head_file(&key_id)?;
            let row = match &spec.at {
                // The served version is the newest *recorded* version whose
                // watermark set `at` covers — a version recorded over
                // sources `at` does not reach never appears in the page
                // (AC-R-2.10.5-5), while a rescore over covered sources
                // does supersede (versions are the durable record).
                Some(at) => match h
                    .versions
                    .iter()
                    .rev()
                    .find(|v| at.covers(&v.watermark_set))
                {
                    Some(v) => self.load_row(&key_id, &v.version_id)?,
                    None => continue,
                },
                None => self.load_row(&key_id, &h.head)?,
            };
            if spec.matches(&row)? {
                rows.push(row);
            }
        }
        rows.sort_by(|a, b| a.key.key_id().cmp(&b.key.key_id()));
        if let Some(cursor) = &spec.cursor {
            let pos = rows
                .iter()
                .position(|r| r.key.key_id() == *cursor)
                .ok_or_else(|| ResultsError::UnknownCursor {
                    cursor: cursor.clone(),
                })?;
            rows = rows.split_off(pos + 1);
        }
        let limit = spec.limit.unwrap_or(usize::MAX);
        let next_cursor = if rows.len() > limit {
            rows.truncate(limit);
            rows.last().map(|r| r.key.key_id())
        } else {
            None
        };
        let mut wm = WatermarkSet::new();
        for r in &rows {
            for (run, seq) in &r.watermark_set.runs {
                wm.pin(run, *seq);
            }
        }
        let view = Json::obj([
            (
                "rows",
                Json::Arr(rows.iter().map(|r| r.to_json()).collect()),
            ),
            ("watermark_set", wm.to_json()),
        ]);
        Ok(Page {
            rows,
            next_cursor,
            watermark_set: wm,
            view_hash: hh_identity::idp::idp_id(
                "results_view",
                view.to_canonical_string().as_bytes(),
            ),
        })
    }

    /// `cells(design_ref | experiment_run_id, at?)` — the `build_cells` read
    /// surface (the table is re-derived per call at the watermark — the
    /// cell table is a view, never a mutable store).
    pub fn cells(
        &self,
        store: &Store,
        docs: &LabDocs,
        target: &str,
        at: Option<&WatermarkSet>,
    ) -> Result<crate::cells::CellTable, ResultsError> {
        crate::cells::build_cells(self, store, docs, target, at)
    }

    /// `distribution(configuration_id, metric_ref, split, at?)` —
    /// `DistributionRef` over the head rows visible at `at` (§6.5 §2.2).
    pub fn distribution(
        &self,
        store: &Store,
        configuration_id: &str,
        metric_ref: &str,
        split: Option<&str>,
        at: Option<&WatermarkSet>,
    ) -> Result<crate::cells::DistributionRef, ResultsError> {
        crate::cells::distribution(self, store, configuration_id, metric_ref, split, at)
    }

    /// `verify_row(version_id)` — re-project the stored version's declared
    /// inputs and compare identities (§6.5 §2.2 `ok | Mismatch{recomputed}`).
    pub fn verify_row(
        &self,
        store: &Store,
        docs: Option<&LabDocs>,
        version_id: &str,
    ) -> Result<VerifyVerdict, ResultsError> {
        let stored = self.find_version(version_id)?;
        let projected = self.project_row(
            store,
            docs,
            &stored.derived_from.run_id,
            Some(stored.derived_from.seq),
            Some(&stored.scoring),
        )?;
        if projected.version_id == stored.version_id {
            Ok(VerifyVerdict::Ok)
        } else {
            Ok(VerifyVerdict::Mismatch {
                recomputed: projected.version_id,
            })
        }
    }

    /// `verify_citation(audit_ref)` — see [`crate::audit::verify_citation`].
    pub fn verify_citation(
        &self,
        store: &Store,
        audit_ref: &crate::audit::AuditRef,
    ) -> crate::audit::CitationVerdict {
        crate::audit::verify_citation(store, audit_ref)
    }

    // ── catalogue + annotations ────────────────────────────────────────

    /// `catalogue_refresh(bundle_ids | all)` — rebuild (or merge) the
    /// header-only bundle catalogue; persists `catalogue.json` with its
    /// watermark set and view hash (§6.5 §2.1; ADR-0161 D3).
    pub fn catalogue_refresh(
        &self,
        store: &Store,
        bundle_ids: Option<&[String]>,
    ) -> Result<crate::catalogue::BundleCatalogue, ResultsError> {
        crate::catalogue::refresh(self, store, bundle_ids)
    }

    /// The persisted catalogue (empty when never refreshed).
    pub fn catalogue(&self) -> Result<crate::catalogue::BundleCatalogue, ResultsError> {
        crate::catalogue::load(self)
    }

    /// `bundle_refs(run_id)` — the catalogue entries citing the run (the
    /// `RowAnnotations.bundle_refs[]` source; ADR-0161 D3).
    pub fn bundle_refs(
        &self,
        run_id: &str,
    ) -> Result<Vec<crate::catalogue::BundleCatalogueEntry>, ResultsError> {
        Ok(self
            .catalogue()?
            .entries
            .into_iter()
            .filter(|e| e.subject_runs.iter().any(|r| r == run_id))
            .collect())
    }

    /// `annotate(scope)` — rebuild the annotation index over the catalogue +
    /// experiment exclusions + the retained snapshot membership (the C1
    /// extension lands `leaders[]`/`scoring_marks[]` — ADR-0161 D3).
    /// Committed indexes emit `annotation_changed` on the journal.
    pub fn annotate(
        &self,
        store: &Store,
        scope: Option<&[String]>,
    ) -> Result<annotations::AnnotationIndex, ResultsError> {
        let idx = annotations::annotate(self, store, scope)?;
        self.journal_emit(crate::journal::JournalEvent {
            kind: crate::journal::JournalKind::AnnotationChanged,
            subject: idx.view_hash.clone(),
            detail: format!("{} entries", idx.entries.len()),
        });
        Ok(idx)
    }

    /// `export_rows(query, target, policy)` — the export op (§6.5 §2.2):
    /// see [`crate::export`]. `measurement.export.delivered` lands on
    /// every cited run; foreign targets produce a `LoweringLossReport`.
    pub fn export_rows(
        &self,
        store: &mut Store,
        rows: &[ResultsRow],
        target: crate::export::ExportTarget,
        policy: &Json,
    ) -> Result<crate::export::ExportOutcome, ResultsError> {
        crate::export::export_rows(self, store, rows, target, policy)
    }

    /// `leaderboard(definition, at?)` — the lab-internal L1–L5 view (pure;
    /// `snapshot_id = view_hash`; no publication at Stage 3 — ADR-0163).
    /// `docs` resolves the design's arms/`MatchSpec`s for the L3 strata.
    pub fn leaderboard(
        &self,
        store: &Store,
        docs: &LabDocs,
        definition: &crate::leaderboard::LeaderboardDefinition,
        at: Option<&WatermarkSet>,
    ) -> Result<crate::leaderboard::LeaderboardSnapshot, ResultsError> {
        crate::leaderboard::leaderboard(self, store, docs, definition, at)
    }

    // ── rebuild ────────────────────────────────────────────────────────

    /// Rebuild the whole derived state from the authoritative sources —
    /// every `agent` run at head (`initial` versions) plus a full catalogue
    /// refresh (AC-R-2.10.5-1's rebuild path: equal inputs ⇒ equal bytes).
    pub fn rebuild_all(
        &self,
        store: &Store,
        docs: Option<&LabDocs>,
    ) -> Result<usize, ResultsError> {
        let mut n = 0;
        for run_id in store.run_ids() {
            let manifest = match store.manifest(&run_id) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if manifest.run_kind != hh_ledger::manifest::RunKind::Agent {
                continue;
            }
            self.project_and_record(store, docs, &run_id, None, None, DerivedReason::Initial)?;
            n += 1;
        }
        self.catalogue_refresh(store, None)?;
        Ok(n)
    }
}
