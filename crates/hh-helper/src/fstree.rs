//! `fs_tree` snapshots — the content-addressed whole-tree state (§5a.5
//! `snapshot(kind)` row; ADR-0137; S2.1). One `tree/1` `ContentAddress` covers
//! every covered root: regular files by blob address + the executable bit,
//! symlinks by target, empty directories legal, deterministic order — the one
//! tree rule (`hh_identity::tree`; CC1/CC7 reuse, never reimplemented).
//!
//! Equal covered state ⇒ equal `snapshot_ref` (AC-R-2.2.5-4): the snapshot
//! ref *is* the tree address. The manifest is the materialisation map —
//! `restore` replays it (file blobs through the ledger blob pool; dirs /
//! symlinks / exec bits on the fs), `verify` recomputes the tree and compares
//! the address.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hh_identity::tree::Tree;
use hh_wire::json::Json;

/// One manifest entry — the materialisable form of a tree member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsNode {
    /// A regular file — `ca` is the blob-pool content address.
    File {
        /// The blob address (`sha256:…` — `hh_identity::address` bytes).
        ca: String,
        /// The executable bit (part of tree identity).
        exec: bool,
        /// The file size.
        size: u64,
    },
    /// A symlink (addressed by target).
    Symlink {
        /// The link target (verbatim).
        target: String,
    },
    /// A directory (possibly empty).
    Dir,
}

/// `FsTreeSnapshot` — a taken tree + its flat manifest (`relpath → node`).
/// `relpath`s are relative to the covered root (`root` recorded separately —
/// a multi-root snapshot is one `Manifest` per root in the covering order).
#[derive(Debug, Clone, PartialEq)]
pub struct FsTreeSnapshot {
    /// The `tree/1` address (the snapshot's `snapshot_ref`).
    pub tree_address: String,
    /// `root → relpath → node` — insertion-ordered (BTreeMap, canonical).
    pub manifest: BTreeMap<String, BTreeMap<String, FsNode>>,
    /// The covered roots (canonical spellings, in coverage order).
    pub roots: Vec<String>,
    /// The payload bytes covered (Σ file sizes).
    pub size_bytes: u64,
}

impl FsTreeSnapshot {
    /// The manifest's canonical member form (the `SnapshotRecord.content`
    /// for `fs_tree`).
    pub fn manifest_json(&self) -> Json {
        Json::obj([
            ("tree_address", Json::str(self.tree_address.clone())),
            (
                "roots",
                Json::Arr(self.roots.iter().map(|r| Json::str(r.clone())).collect()),
            ),
            (
                "manifest",
                Json::Obj(
                    self.manifest
                        .iter()
                        .map(|(root, entries)| {
                            (
                                root.clone(),
                                Json::Obj(
                                    entries
                                        .iter()
                                        .map(|(rel, n)| (rel.clone(), node_json(n)))
                                        .collect(),
                                ),
                            )
                        })
                        .collect(),
                ),
            ),
            ("size_bytes", Json::Int(self.size_bytes as i64)),
        ])
    }

    /// Decode a manifest produced by `manifest_json` (strict member shapes).
    pub fn from_manifest_json(j: &Json) -> Option<FsTreeSnapshot> {
        let tree_address = j.get("tree_address")?.as_str()?.to_string();
        let roots: Vec<String> = match j.get("roots")? {
            Json::Arr(a) => a
                .iter()
                .map(|r| r.as_str().map(String::from))
                .collect::<Option<_>>()?,
            _ => return None,
        };
        let mut manifest = BTreeMap::new();
        if let Json::Obj(roots_obj) = j.get("manifest")? {
            for (root, entries) in roots_obj {
                let mut map = BTreeMap::new();
                if let Json::Obj(es) = entries {
                    for (rel, nj) in es {
                        map.insert(rel.clone(), node_from_json(nj)?);
                    }
                }
                manifest.insert(root.clone(), map);
            }
        }
        Some(FsTreeSnapshot {
            tree_address,
            manifest,
            roots,
            size_bytes: j.get("size_bytes")?.as_int()? as u64,
        })
    }
}

fn node_json(n: &FsNode) -> Json {
    match n {
        FsNode::File { ca, exec, size } => Json::obj([
            ("kind", Json::str("file")),
            ("ca", Json::str(ca.clone())),
            ("exec", Json::Bool(*exec)),
            ("size", Json::Int(*size as i64)),
        ]),
        FsNode::Symlink { target } => Json::obj([
            ("kind", Json::str("symlink")),
            ("target", Json::str(target.clone())),
        ]),
        FsNode::Dir => Json::obj([("kind", Json::str("dir"))]),
    }
}

fn node_from_json(j: &Json) -> Option<FsNode> {
    match j.get("kind")?.as_str()? {
        "file" => Some(FsNode::File {
            ca: j.get("ca")?.as_str()?.to_string(),
            exec: matches!(j.get("exec"), Some(Json::Bool(true))),
            size: j.get("size")?.as_int()? as u64,
        }),
        "symlink" => Some(FsNode::Symlink {
            target: j.get("target")?.as_str()?.to_string(),
        }),
        "dir" => Some(FsNode::Dir),
        _ => None,
    }
}

/// One manifest-vs-manifest difference (the `diff` row; deterministic order —
/// the manifest is a `BTreeMap` and entries sort by `(root, relpath)`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FsTreeDiffEntry {
    /// The covered root.
    pub root: String,
    /// The relative path.
    pub relpath: String,
    /// `added | removed | modified`.
    pub change: &'static str,
    /// The before identity (content address / target / `dir`).
    pub before: Option<String>,
    /// The after identity.
    pub after: Option<String>,
}

fn node_id(n: &FsNode) -> String {
    match n {
        FsNode::File { ca, exec, .. } => format!("file:{ca}{}", if *exec { ":x" } else { "" }),
        FsNode::Symlink { target } => format!("symlink:{target}"),
        FsNode::Dir => "dir".to_string(),
    }
}

/// `walk(roots)` — take the `fs_tree` over `roots` (canonical order). A
/// root that does not exist contributes an empty manifest (honest — the
/// covered state is empty). Unreadable entries are skipped only when the
/// fs itself refuses them mid-walk (recorded as absent — the diff surface
/// sees them as `removed`); non-UTF-8 names are carried verbatim through
/// `String::from_utf8_lossy` (the tree rule's string form).
pub fn walk(roots: &[String]) -> std::io::Result<FsTreeSnapshot> {
    let mut manifest: BTreeMap<String, BTreeMap<String, FsNode>> = BTreeMap::new();
    let mut total = 0u64;
    let mut trees: Vec<Tree> = Vec::new();
    for root in roots {
        let mut entries: BTreeMap<String, FsNode> = BTreeMap::new();
        let t = walk_dir(Path::new(root), Path::new(root), &mut entries, &mut total)?;
        manifest.insert(root.clone(), entries);
        trees.push(t);
    }
    // The covering tree: one `Dir` member per covered root, keyed by the
    // root's *position* (`r0`, `r1`, …) — never by path. Equal covered state
    // at different paths ⇒ equal address (AC-R-2.2.5-4: byte-identical
    // workspaces under different handles yield equal `snapshot_ref`).
    let mut top = Tree::new();
    for (i, t) in trees.into_iter().enumerate() {
        top = top.dir(&format!("r{i}"), t);
    }
    Ok(FsTreeSnapshot {
        tree_address: top.address().id(),
        manifest,
        roots: roots.to_vec(),
        size_bytes: total,
    })
}

/// Recursively walk `dir`, recording entries under `rel` (relative to the
/// covered root) and folding the subtree.
fn walk_dir(
    root: &Path,
    dir: &Path,
    entries: &mut BTreeMap<String, FsNode>,
    total: &mut u64,
) -> std::io::Result<Tree> {
    let mut t = Tree::new();
    let rel_of = |p: &Path| -> String {
        p.strip_prefix(root)
            .unwrap_or(p)
            .to_string_lossy()
            .to_string()
    };
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => {
            // The directory itself is a manifest member (empty when
            // unreadable/vanished — honest absent state).
            if dir != root {
                entries.insert(rel_of(dir), FsNode::Dir);
            }
            return Ok(t);
        }
    };
    if dir != root {
        entries.insert(rel_of(dir), FsNode::Dir);
    }
    let mut names: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    names.sort();
    for p in names {
        let md = match std::fs::symlink_metadata(&p) {
            Ok(m) => m,
            Err(_) => continue, // vanished mid-walk — absent state
        };
        let name = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let rel = rel_of(&p);
        if md.is_symlink() {
            let target = std::fs::read_link(&p)
                .map(|t| t.to_string_lossy().to_string())
                .unwrap_or_default();
            entries.insert(
                rel,
                FsNode::Symlink {
                    target: target.clone(),
                },
            );
            t = t.symlink(&name, &target);
        } else if md.is_dir() {
            let sub = walk_dir(root, &p, entries, total)?;
            t = t.dir(&name, sub);
        } else if md.is_file() {
            let content = std::fs::read(&p).unwrap_or_default();
            *total = total.saturating_add(content.len() as u64);
            let exec = is_executable(&md);
            let ca = hh_identity::idp::address(&content, "application/octet-stream").id();
            entries.insert(
                rel,
                FsNode::File {
                    ca,
                    exec,
                    size: content.len() as u64,
                },
            );
            t = if exec {
                t.exec(&name, &content)
            } else {
                t.file(&name, &content)
            };
        }
        // fifos/sockets/devices carry no fs_tree member — the tree rule's
        // sum is {file, symlink, dir} (recorded absent, honestly).
    }
    Ok(t)
}

#[cfg(unix)]
fn is_executable(md: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    md.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_md: &std::fs::Metadata) -> bool {
    false
}

/// `verify(snapshot)` — recompute the tree over the covered roots; the
/// address must match (the `verify` half of `snapshot/restore/verify`).
pub fn verify(snap: &FsTreeSnapshot) -> std::io::Result<bool> {
    let now = walk(&snap.roots)?;
    Ok(now.tree_address == snap.tree_address)
}

/// `diff(before, after)` — the deterministic change-set between two
/// manifests (sorted by `(root, relpath)`).
pub fn diff(before: &FsTreeSnapshot, after: &FsTreeSnapshot) -> Vec<FsTreeDiffEntry> {
    let mut out = Vec::new();
    let mut roots: Vec<&String> = before
        .manifest
        .keys()
        .chain(after.manifest.keys())
        .collect();
    roots.sort();
    roots.dedup();
    for root in roots {
        let b = before.manifest.get(root);
        let a = after.manifest.get(root);
        let mut paths: Vec<&String> = b
            .iter()
            .flat_map(|m| m.keys())
            .chain(a.iter().flat_map(|m| m.keys()))
            .collect();
        paths.sort();
        paths.dedup();
        for rel in paths {
            let bn = b.and_then(|m| m.get(rel));
            let an = a.and_then(|m| m.get(rel));
            match (bn, an) {
                (None, Some(an)) => out.push(FsTreeDiffEntry {
                    root: (*root).clone(),
                    relpath: rel.clone(),
                    change: "added",
                    before: None,
                    after: Some(node_id(an)),
                }),
                (Some(bn), None) => out.push(FsTreeDiffEntry {
                    root: (*root).clone(),
                    relpath: rel.clone(),
                    change: "removed",
                    before: Some(node_id(bn)),
                    after: None,
                }),
                (Some(bn), Some(an)) if node_id(bn) != node_id(an) => out.push(FsTreeDiffEntry {
                    root: (*root).clone(),
                    relpath: rel.clone(),
                    change: "modified",
                    before: Some(node_id(bn)),
                    after: Some(node_id(an)),
                }),
                _ => {}
            }
        }
    }
    out
}

/// `restore(snapshot, dest_root, blob_read)` — materialise the manifest under
/// `dest_root` (which must be empty-or-absent — the successor workspace).
/// `blob_read` resolves a content address to bytes (the ledger blob pool
/// kernel-side; the helper's own pool verb-side). Returns the recomputed
/// tree address — the caller compares it to `tree_address` (a mismatch is a
/// `verify` failure, never silently accepted).
pub fn restore(
    snap: &FsTreeSnapshot,
    dest_root: &Path,
    blob_read: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> std::io::Result<String> {
    for (root, entries) in &snap.manifest {
        // The successor's covered root maps to dest_root (a restore yields a
        // fresh workspace — the covered roots are replays into one tree).
        let _ = root;
        for (rel, node) in entries {
            let dest = dest_root.join(rel);
            match node {
                FsNode::Dir => {
                    std::fs::create_dir_all(&dest)?;
                }
                FsNode::File { ca, exec, .. } => {
                    if let Some(parent) = dest.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    let bytes = blob_read(ca).ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            format!("blob {ca} missing"),
                        )
                    })?;
                    std::fs::write(&dest, &bytes)?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let mode = if *exec { 0o755 } else { 0o644 };
                        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(mode))?;
                    }
                    let _ = exec;
                }
                FsNode::Symlink { target } => {
                    if let Some(parent) = dest.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    let _ = std::fs::remove_file(&dest);
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(target, &dest)?;
                    #[cfg(not(unix))]
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Unsupported,
                        "symlink restore unsupported on this platform",
                    ));
                }
            }
        }
    }
    // Recompute over the restored root — a single covered root re-keys to
    // `r0`, matching a one-root snapshot's address exactly.
    let check = walk(&[dest_root.to_string_lossy().to_string()])?;
    Ok(check.tree_address)
}

/// The content-addressed blob helper — `address(bytes)` (the blob pool id).
pub fn content_address(bytes: &[u8]) -> String {
    hh_identity::idp::address(bytes, "application/octet-stream").id()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_trees_have_equal_addresses() {
        let d1 = std::env::temp_dir().join(format!("hh-fst-{}", std::process::id()));
        let a = d1.join("a");
        let b = d1.join("b");
        std::fs::create_dir_all(a.join("sub/empty")).unwrap();
        std::fs::create_dir_all(b.join("sub/empty")).unwrap();
        std::fs::write(a.join("f.txt"), b"hello").unwrap();
        std::fs::write(b.join("f.txt"), b"hello").unwrap();
        let sa = walk(&[a.to_string_lossy().to_string()]).unwrap();
        let sb = walk(&[b.to_string_lossy().to_string()]).unwrap();
        // Positional covering keys — byte-identical workspaces at different
        // paths yield equal addresses (AC-R-2.2.5-4).
        let sa2 = walk(&[a.to_string_lossy().to_string()]).unwrap();
        assert_eq!(sa.tree_address, sa2.tree_address);
        assert_eq!(sa.tree_address, sb.tree_address);
        std::fs::write(b.join("g.txt"), b"extra").unwrap();
        let sb2 = walk(&[b.to_string_lossy().to_string()]).unwrap();
        assert_ne!(sa.tree_address, sb2.tree_address);
        let _ = std::fs::remove_dir_all(&d1);
    }

    #[test]
    fn diff_is_deterministic() {
        let d = std::env::temp_dir().join(format!("hh-fsd-{}", std::process::id()));
        let r = d.join("w");
        std::fs::create_dir_all(&r).unwrap();
        std::fs::write(r.join("a.txt"), b"a").unwrap();
        let s1 = walk(&[r.to_string_lossy().to_string()]).unwrap();
        std::fs::write(r.join("b.txt"), b"b").unwrap();
        std::fs::remove_file(r.join("a.txt")).unwrap();
        let s2 = walk(&[r.to_string_lossy().to_string()]).unwrap();
        let diff = diff(&s1, &s2);
        assert_eq!(diff.len(), 2);
        assert_eq!(diff[0].change, "removed");
        assert_eq!(diff[1].change, "added");
        let _ = std::fs::remove_dir_all(&d);
    }
}
