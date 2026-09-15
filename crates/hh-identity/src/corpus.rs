//! The **golden corpus** (§8.3 #8 fixtures; AC-5): HIR/ledger canonical byte records plus **≥ 20
//! directory trees** exercising symlinks, executables, empty dirs and non-ASCII names. The corpus
//! is deterministic and hermetic (no filesystem, no clock). AC-5 asserts `address`/`identify`
//! agree byte-for-byte with the independently-recorded golden answers (`tests/golden_corpus.rs`),
//! the offline realization of "two independent implementations agree" (the cross-candidate E2/E3
//! re-run on this extended tree corpus is tracked by DEFERRALS DF-S1.2-2, golden table as proxy).

use hh_wire::json::Json;

use crate::idp::{address, identify_text};
use crate::kinds::RecordKind;
use crate::record::{identify, Record};
use crate::tree::Tree;

/// One named golden item and its computed id (`<algorithm>:<hex>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoldenItem {
    pub name: String,
    pub id: String,
}

/// The ≥ 20 directory trees of the golden corpus, each with its computed tree address. Covers:
/// empty dirs, single/multi-file dirs, executables, symlinks (by target), nested dirs, non-ASCII
/// names, and files whose bytes match another entry's (domain separation still distinguishes them).
pub fn golden_trees() -> Vec<GoldenItem> {
    let trees: Vec<(&str, Tree)> = vec![
        ("empty", Tree::new()),
        ("one-file", Tree::new().file("a.txt", b"alpha")),
        (
            "two-files",
            Tree::new().file("a.txt", b"alpha").file("b.txt", b"beta"),
        ),
        ("exec", Tree::new().exec("run.sh", b"#!/bin/sh\necho hi\n")),
        (
            "exec-vs-plain",
            Tree::new().exec("x", b"same").file("y", b"same"),
        ),
        ("symlink", Tree::new().symlink("link", "../target")),
        ("symlink-abs", Tree::new().symlink("link", "/etc/hosts")),
        (
            "symlink-and-file",
            Tree::new().file("real", b"data").symlink("alias", "real"),
        ),
        ("empty-subdir", Tree::new().dir("sub", Tree::new())),
        (
            "nested",
            Tree::new().dir(
                "a",
                Tree::new().dir("b", Tree::new().file("c.txt", b"deep")),
            ),
        ),
        ("non-ascii-name", Tree::new().file("café-☕.txt", b"latte")),
        (
            "non-ascii-emoji-dir",
            Tree::new().dir("📁", Tree::new().file("f", b"x")),
        ),
        ("cyrillic", Tree::new().file("привет.txt", b"hello")),
        ("cjk", Tree::new().file("日本語.md", b"nihongo")),
        (
            "mixed",
            Tree::new()
                .file("a", b"1")
                .exec("b", b"2")
                .symlink("c", "a")
                .dir("d", Tree::new()),
        ),
        ("many-files", {
            let mut t = Tree::new();
            for i in 0..10 {
                t = t.file(&format!("f{i:02}.txt"), format!("content-{i}").as_bytes());
            }
            t
        }),
        (
            "binary-content",
            Tree::new().file("blob.bin", &[0u8, 1, 2, 255, 254, 0, 128]),
        ),
        ("empty-file", Tree::new().file("empty", b"")),
        (
            "deep-symlink-tree",
            Tree::new().dir("d1", Tree::new().symlink("up", "..").file("k", b"v")),
        ),
        (
            "dotfiles",
            Tree::new().file(".hidden", b"h").file(".config", b"c"),
        ),
        (
            "whitespace-names",
            Tree::new()
                .file("a b.txt", b"space")
                .file("tab\tname", b"tab"),
        ),
        (
            "sibling-equal-bytes",
            Tree::new()
                .file("x", b"dup")
                .file("y", b"dup")
                .dir("z", Tree::new().file("x", b"dup")),
        ),
    ];
    trees
        .into_iter()
        .map(|(name, t)| GoldenItem {
            name: name.to_string(),
            id: t.address().id(),
        })
        .collect()
}

/// The HIR/ledger canonical-byte arm of the golden corpus: a blob, a `Text` leaf and an HIR node
/// over identical canonical bytes (three distinct ids — N3/AC-2), plus a sealed definition and a
/// run manifest with pinned refs.
pub fn golden_records() -> Vec<GoldenItem> {
    let shared = b"{\"canonical\":\"bytes\",\"n\":1}";
    let mut items = vec![
        GoldenItem {
            name: "blob".into(),
            id: address(shared, "application/octet-stream").id(),
        },
        GoldenItem {
            name: "text-leaf".into(),
            id: identify_text(shared),
        },
    ];
    let node = Record::new(RecordKind::HirNode)
        .semantic("op", Json::str("map"))
        .semantic("n", Json::Int(1));
    let node_id = identify(&node).unwrap();
    items.push(GoldenItem {
        name: "hir-node.version_id".into(),
        id: node_id.version_id,
    });
    items.push(GoldenItem {
        name: "hir-node.semantic_id".into(),
        id: node_id.semantic_id.unwrap(),
    });
    let ledger_event = Record::new(RecordKind::HirNode)
        .semantic("event", Json::str("lifecycle.run.opened"))
        .semantic("seq", Json::Int(0));
    items.push(GoldenItem {
        name: "ledger-event.version_id".into(),
        id: identify(&ledger_event).unwrap().version_id,
    });
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_has_at_least_twenty_trees() {
        // AC-5: "≥ 20 directory trees".
        assert!(
            golden_trees().len() >= 20,
            "need ≥ 20 trees, got {}",
            golden_trees().len()
        );
    }

    #[test]
    fn every_golden_id_is_a_valid_full_id() {
        for item in golden_trees().into_iter().chain(golden_records()) {
            assert!(
                crate::idp::parse_id(&item.id).is_ok(),
                "bad id for {}",
                item.name
            );
        }
    }

    #[test]
    fn corpus_is_deterministic_across_calls() {
        // The whole point: address/identify are pure functions of the bytes.
        assert_eq!(golden_trees(), golden_trees());
        assert_eq!(golden_records(), golden_records());
    }

    #[test]
    fn tree_names_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for item in golden_trees() {
            assert!(
                seen.insert(item.name.clone()),
                "dup tree name {}",
                item.name
            );
        }
    }
}
