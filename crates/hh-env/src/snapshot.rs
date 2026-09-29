//! `SnapshotRecord` — the *state* identity layer (ADR-0137 §3): a
//! content-addressed record of the environment's state at a sequence point,
//! addressed under `idp/1`'s `snapshot` domain. At Stage 1 the only supported
//! `SnapshotKind` is `path_baseline` — the readable-root content map the
//! capture path diffs to produce `fs_change` items.
//!
//! `shell_state` is never identity-bearing (S-rules — shell state is a process
//! epiphenomenon, not a content fact); it stays out of `SnapshotRecord`
//! entirely.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use hh_identity::idp::idp_id;
use hh_wire::json::Json;

/// `SnapshotKind` — the closed sum (`path_baseline` is the Stage-1 member).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SnapshotKind {
    /// `{path → content_id}` over the readable roots — the diff baseline.
    PathBaseline,
    /// A whole-tree address (Stage 2).
    FsTree,
    /// A layered filesystem snapshot (Stage 2).
    FsLayer,
    /// Process memory (Stage 3).
    Memory,
    /// Shell state — **never identity-bearing** (S-rules); declared in the sum
    /// so a capability declaration can say `unknown`, never stored.
    ShellState,
}

impl SnapshotKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SnapshotKind::PathBaseline => "path_baseline",
            SnapshotKind::FsTree => "fs_tree",
            SnapshotKind::FsLayer => "fs_layer",
            SnapshotKind::Memory => "memory",
            SnapshotKind::ShellState => "shell_state",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<SnapshotKind> {
        Some(match s {
            "path_baseline" => SnapshotKind::PathBaseline,
            "fs_tree" => SnapshotKind::FsTree,
            "fs_layer" => SnapshotKind::FsLayer,
            "memory" => SnapshotKind::Memory,
            "shell_state" => SnapshotKind::ShellState,
            _ => return None,
        })
    }

    /// Whether the kind may appear in a `SnapshotRecord`'s identity — all but
    /// `shell_state` (S-rules).
    pub fn is_identity_bearing(self) -> bool {
        !matches!(self, SnapshotKind::ShellState)
    }
}

/// `TakenBy` — who took the snapshot (`subject` = the run's workload;
/// `instrument` = the measurement harness — an instrument snapshot is never
/// charged to the subject).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TakenBy {
    /// The run's workload.
    Subject,
    /// The measurement instrument.
    Instrument,
}

/// `SnapshotRecord` — the content-addressed state record (ADR-0137 §3).
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotRecord {
    /// `snapshot_ref` — the `idp/1` content id under the `snapshot` domain.
    pub snapshot_ref: String,
    /// The handle it snapshots.
    pub env_handle_id: String,
    /// The sequence point.
    pub at_seq: u64,
    /// The kind.
    pub kind: SnapshotKind,
    /// `base` — the image `ContentAddress` id, a parent `snapshot_ref`, or
    /// `null` (a full baseline).
    pub base: Option<String>,
    /// `content` — the per-kind payload: `path_baseline` carries the
    /// `[{path, content_id}]` map; `fs_tree` a tree address; `fs_layer` a layer.
    pub content: Json,
    /// The roots the snapshot covers.
    pub roots_covered: Vec<String>,
    /// Whether the environment was quiesced (a Stage-2+ guarantee; `false` is
    /// honest — a live `path_baseline` is a point-in-time read, not atomic).
    pub quiesced: bool,
    /// Who took it.
    pub taken_by: TakenBy,
    /// The byte size.
    pub size_bytes: u64,
    /// Expiry (`None` = run lifetime).
    pub expires_at_ms: Option<u64>,
}

impl SnapshotRecord {
    /// The canonical member form — `snapshot_ref` is derived over it.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("env_handle_id", Json::str(self.env_handle_id.clone())),
            ("at_seq", Json::Int(self.at_seq as i64)),
            ("kind", Json::str(self.kind.as_str())),
            (
                "base",
                match &self.base {
                    Some(b) => Json::str(b.clone()),
                    None => Json::Null,
                },
            ),
            ("content", self.content.clone()),
            (
                "roots_covered",
                Json::Arr(
                    self.roots_covered
                        .iter()
                        .map(|r| Json::str(r.clone()))
                        .collect(),
                ),
            ),
            ("quiesced", Json::Bool(self.quiesced)),
            (
                "taken_by",
                Json::str(match self.taken_by {
                    TakenBy::Subject => "subject",
                    TakenBy::Instrument => "instrument",
                }),
            ),
            ("size_bytes", Json::Int(self.size_bytes as i64)),
            (
                "expires_at_ms",
                match self.expires_at_ms {
                    Some(t) => Json::Int(t as i64),
                    None => Json::Null,
                },
            ),
        ])
    }

    /// Decode the member form — the fork/rollback snapshot chooser reloads a
    /// record from its `manifest_ref` blob (S2.9; the row itself carries only
    /// `{env_handle, snapshot_ref, kind, at_seq, manifest_ref}`).
    pub fn from_json(j: &Json) -> Option<SnapshotRecord> {
        let kind = SnapshotKind::parse(j.get("kind")?.as_str()?)?;
        let taken_by = match j.get("taken_by")?.as_str()? {
            "subject" => TakenBy::Subject,
            "instrument" => TakenBy::Instrument,
            _ => return None,
        };
        let mut rec = SnapshotRecord {
            snapshot_ref: String::new(),
            env_handle_id: j.get("env_handle_id")?.as_str()?.to_string(),
            at_seq: j.get("at_seq")?.as_int()? as u64,
            kind,
            base: j.get("base").and_then(Json::as_str).map(str::to_string),
            content: j.get("content")?.clone(),
            roots_covered: match j.get("roots_covered")? {
                Json::Arr(a) => a
                    .iter()
                    .map(|r| r.as_str().map(String::from))
                    .collect::<Option<_>>()?,
                _ => return None,
            },
            quiesced: matches!(j.get("quiesced"), Some(Json::Bool(true))),
            taken_by,
            size_bytes: j.get("size_bytes")?.as_int()? as u64,
            expires_at_ms: j
                .get("expires_at_ms")
                .and_then(Json::as_int)
                .map(|t| t as u64),
        };
        // The ref is derived — never carried (the row's copy is the same
        // derivation; a blob whose members disagree recomputes honestly).
        rec.snapshot_ref = rec.compute_ref();
        Some(rec)
    }

    /// The `snapshot_ref` — `idp/1` under the `snapshot` domain over the
    /// canonical member form (excluding `snapshot_ref` itself — it *is* the
    /// derived id). `at_seq` is provenance — *where the observation landed*,
    /// not what was observed — so it is excluded from the basis: two
    /// snapshots of an unchanged workspace share one address
    /// (AC-R-2.2.4-9's dedup; the `at_seq` still rides the member form and
    /// the row, and the fork/rollback chooser reads it row-side).
    pub fn compute_ref(&self) -> String {
        let mut j = self.to_json();
        if let Json::Obj(m) = &mut j {
            m.remove("at_seq");
        }
        idp_id("snapshot", j.to_canonical_string().as_bytes())
    }
}

/// `PathBaseline` — the `{canonical_path → content_id}` map over the readable
/// roots. Built by walking the roots; diffed against a post-exec baseline to
/// produce the capture's `fs_change` items (the kernel's own observer for the
/// `fs_write` domain — `fs_change` is *observed*, never self-reported).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PathBaseline {
    /// `canonical_path → content_id` (`sha256:<hex>` under the `blob` domain).
    pub entries: BTreeMap<String, String>,
}

impl PathBaseline {
    /// Build a baseline by reading each `paths` entry through `read` — `read`
    /// returns the file's bytes (`None` = absent/unreadable). A directory is
    /// expanded by the caller into its file list.
    pub fn take(read: &dyn Fn(&str) -> Option<Vec<u8>>, paths: &[String]) -> PathBaseline {
        let mut entries = BTreeMap::new();
        for p in paths {
            if let Some(bytes) = read(p) {
                let ca = hh_identity::idp::address(&bytes, "application/octet-stream");
                entries.insert(p.clone(), ca.id());
            }
        }
        PathBaseline { entries }
    }

    /// The `content` member for a `path_baseline` `SnapshotRecord`.
    pub fn to_json(&self) -> Json {
        Json::Arr(
            self.entries
                .iter()
                .map(|(p, id)| {
                    Json::obj([
                        ("path", Json::str(p.clone())),
                        ("ca", Json::str(id.clone())),
                    ])
                })
                .collect(),
        )
    }

    /// `diff(before, after)` — the `FsChangeSet` (added/modified/removed by
    /// canonical path; `before_ref`/`after_ref` are the content ids).
    pub fn diff(before: &PathBaseline, after: &PathBaseline) -> FsChangeSet {
        let mut added = Vec::new();
        let mut modified = Vec::new();
        let mut removed = Vec::new();
        for (p, a) in &after.entries {
            match before.entries.get(p) {
                None => added.push(FsChangeEntry {
                    path_canonical: p.clone(),
                    before_ref: None,
                    after_ref: Some(a.clone()),
                }),
                Some(b) if b != a => modified.push(FsChangeEntry {
                    path_canonical: p.clone(),
                    before_ref: Some(b.clone()),
                    after_ref: Some(a.clone()),
                }),
                _ => {}
            }
        }
        for (p, b) in &before.entries {
            if !after.entries.contains_key(p) {
                removed.push(FsChangeEntry {
                    path_canonical: p.clone(),
                    before_ref: Some(b.clone()),
                    after_ref: None,
                });
            }
        }
        FsChangeSet {
            added,
            modified,
            removed,
        }
    }
}

/// `FsChangeEntry` — one path's before/after content ids.
#[derive(Debug, Clone, PartialEq)]
pub struct FsChangeEntry {
    /// The canonical path.
    pub path_canonical: String,
    /// The before content id (`None` = added).
    pub before_ref: Option<String>,
    /// The after content id (`None` = removed).
    pub after_ref: Option<String>,
}

/// `FsChangeSet` — the `path_baseline` diff (`added ∪ modified ∪ removed`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FsChangeSet {
    /// Paths that appeared.
    pub added: Vec<FsChangeEntry>,
    /// Paths whose content changed.
    pub modified: Vec<FsChangeEntry>,
    /// Paths that disappeared.
    pub removed: Vec<FsChangeEntry>,
}

impl FsChangeSet {
    /// All entries (added, then modified, then removed — the canonical order).
    pub fn entries(&self) -> impl Iterator<Item = (&FsChangeEntry, &'static str)> {
        self.added
            .iter()
            .map(|e| (e, "added"))
            .chain(self.modified.iter().map(|e| (e, "modified")))
            .chain(self.removed.iter().map(|e| (e, "removed")))
    }

    /// Whether the diff is empty.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.modified.is_empty() && self.removed.is_empty()
    }

    /// The paths the diff touched (for the writable-root check).
    pub fn paths(&self) -> BTreeSet<&str> {
        self.entries()
            .map(|(e, _)| e.path_canonical.as_str())
            .collect()
    }
}
