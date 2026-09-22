//! `address` over directory **trees** — the one tree rule (§8.3 #2, N8; ADR-0036 D1/D3).
//!
//! A hermetic, in-memory tree model (no filesystem, no clock — deterministic and offline). The
//! one tree rule: entries are ordered by name (bytewise), symlinks are addressed **by target**
//! (not by pointee content), regular files carry an executable bit, empty directories are legal,
//! non-ASCII names are carried verbatim (the canonical JSON string form escapes them), and there
//! are **no timestamps or ownership**. A directory hashed under `idp/1` is one `ContentAddress`
//! with domain tag `tree`; a foreign digest of the same tree would be a claim, never this
//! identity (N8).

use std::collections::BTreeMap;

use hh_wire::json::Json;

use crate::idp::{address, idp_id, ContentAddress, IDP_1};

/// A single tree entry (the value of a name → entry mapping).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// A regular file: content bytes + the executable bit.
    File { content: Vec<u8>, executable: bool },
    /// A symlink, addressed by its target string (not by the target's content).
    Symlink { target: String },
    /// A subdirectory.
    Dir(Tree),
}

/// A directory tree: names → entries. `BTreeMap` gives the bytewise-sorted order the tree rule
/// requires, by construction.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tree {
    entries: BTreeMap<String, Entry>,
}

impl Tree {
    pub fn new() -> Tree {
        Tree {
            entries: BTreeMap::new(),
        }
    }

    /// Add a regular file.
    pub fn file(mut self, name: &str, content: &[u8]) -> Tree {
        self.entries.insert(
            name.to_string(),
            Entry::File {
                content: content.to_vec(),
                executable: false,
            },
        );
        self
    }

    /// Add an executable file (the exec bit is part of the tree identity).
    pub fn exec(mut self, name: &str, content: &[u8]) -> Tree {
        self.entries.insert(
            name.to_string(),
            Entry::File {
                content: content.to_vec(),
                executable: true,
            },
        );
        self
    }

    /// Add a symlink (addressed by target).
    pub fn symlink(mut self, name: &str, target: &str) -> Tree {
        self.entries.insert(
            name.to_string(),
            Entry::Symlink {
                target: target.to_string(),
            },
        );
        self
    }

    /// Add a subdirectory (may be empty).
    pub fn dir(mut self, name: &str, tree: Tree) -> Tree {
        self.entries.insert(name.to_string(), Entry::Dir(tree));
        self
    }

    /// The canonical JSON of this tree under the one tree rule: each entry rendered by its kind,
    /// files by their blob `ContentAddress` id, symlinks by target, subdirs by their tree id.
    /// Sorted keys (BTreeMap) make it deterministic.
    fn canonical(&self) -> Json {
        let mut pairs: BTreeMap<String, Json> = BTreeMap::new();
        for (name, entry) in &self.entries {
            let v = match entry {
                Entry::File {
                    content,
                    executable,
                } => Json::obj([
                    ("kind", Json::str("file")),
                    ("exec", Json::Bool(*executable)),
                    (
                        "content",
                        Json::str(address(content, "application/octet-stream").id()),
                    ),
                ]),
                Entry::Symlink { target } => Json::obj([
                    ("kind", Json::str("symlink")),
                    ("target", Json::str(target.clone())),
                ]),
                Entry::Dir(sub) => Json::obj([
                    ("kind", Json::str("dir")),
                    ("tree", Json::str(sub.address().id())),
                ]),
            };
            pairs.insert(name.clone(), v);
        }
        Json::Obj(pairs)
    }

    /// `address(tree) → ContentAddress` under domain tag `tree` (§8.3 #2). Idempotent and
    /// recursive; a subtree's address is embedded in its parent's canonical form.
    pub fn address(&self) -> ContentAddress {
        let bytes = self.canonical().to_canonical_string().into_bytes();
        let digest = idp_id("tree", &bytes);
        // `idp_id` returns `<algorithm>:<hex>`; split back into the ContentAddress fields.
        let hex = digest.strip_prefix("sha256:").unwrap().to_string();
        ContentAddress {
            idp: IDP_1.idp_id,
            algorithm: IDP_1.hash_algorithm,
            digest: hex,
            media_type: "application/vnd.hh.tree".to_string(),
            size: bytes.len() as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_dir_has_a_stable_address() {
        let a = Tree::new().address();
        let b = Tree::new().address();
        assert_eq!(a.id(), b.id());
        assert!(a.id().starts_with("sha256:"));
    }

    #[test]
    fn exec_bit_changes_the_tree_address() {
        let plain = Tree::new().file("run.sh", b"#!/bin/sh\n").address();
        let exe = Tree::new().exec("run.sh", b"#!/bin/sh\n").address();
        assert_ne!(
            plain.id(),
            exe.id(),
            "the executable bit is part of tree identity"
        );
    }

    #[test]
    fn symlink_is_addressed_by_target_not_content() {
        let a = Tree::new().symlink("link", "../a").address();
        let b = Tree::new().symlink("link", "../b").address();
        assert_ne!(a.id(), b.id());
    }

    #[test]
    fn non_ascii_names_are_carried_verbatim() {
        let a = Tree::new().file("café-☕.txt", b"x").address();
        let b = Tree::new().file("cafe.txt", b"x").address();
        assert_ne!(a.id(), b.id());
        // deterministic
        assert_eq!(a.id(), Tree::new().file("café-☕.txt", b"x").address().id());
    }

    #[test]
    fn nested_dirs_recurse() {
        let inner = Tree::new().file("a.txt", b"a");
        let outer = Tree::new().dir("sub", inner.clone());
        assert_eq!(
            outer.address().id(),
            Tree::new().dir("sub", inner).address().id()
        );
        // A change deep in the tree changes the root address.
        let outer2 = Tree::new().dir("sub", Tree::new().file("a.txt", b"b"));
        assert_ne!(outer.address().id(), outer2.address().id());
    }
}
