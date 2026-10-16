//! The tree-manifest codec — the language-neutral encoding of a
//! `hh_identity::tree::Tree` the replay bundle carries as the *input* a
//! foreign implementation addresses (DF-S1.2-2).
//!
//! Manifest form (canonical JSON):
//! `{ "<name>": { "kind": "file", "exec": bool, "bytes_hex": "<hex>" }
//!             | { "kind": "symlink", "target": "<string>" }
//!             | { "kind": "dir", "entries": { …recurse… } } }`
//!
//! File bytes ride `bytes_hex` so a manifest is pure canonical JSON.

use hh_identity::tree::{Entry, Tree};
use hh_wire::json::Json;

/// `Tree → manifest Json` (the bundle's `<name>.input.json` member).
pub fn tree_manifest(tree: &Tree) -> Json {
    let mut m = std::collections::BTreeMap::new();
    for (name, entry) in tree.entries() {
        let v = match entry {
            Entry::File {
                content,
                executable,
            } => Json::obj([
                ("bytes_hex", Json::str(crate::hex::encode(content))),
                ("exec", Json::Bool(*executable)),
                ("kind", Json::str("file")),
            ]),
            Entry::Symlink { target } => Json::obj([
                ("kind", Json::str("symlink")),
                ("target", Json::str(target.clone())),
            ]),
            Entry::Dir(sub) => {
                Json::obj([("entries", tree_manifest(sub)), ("kind", Json::str("dir"))])
            }
        };
        m.insert(name.clone(), v);
    }
    Json::Obj(m)
}

/// `manifest Json → Tree` — the E1 self-check's decode leg (and the shape a
/// foreign implementation reads). Fails typed on a malformed member.
pub fn manifest_to_tree(j: &Json) -> Result<Tree, String> {
    let Json::Obj(entries) = j else {
        return Err("tree manifest must be an object".to_string());
    };
    let mut tree = Tree::new();
    for (name, v) in entries {
        let kind = v
            .get("kind")
            .and_then(Json::as_str)
            .ok_or_else(|| format!("{name}: missing kind"))?;
        match kind {
            "file" => {
                let bytes_hex = v
                    .get("bytes_hex")
                    .and_then(Json::as_str)
                    .ok_or_else(|| format!("{name}: missing bytes_hex"))?;
                let bytes = crate::hex::decode(bytes_hex)?;
                let exec = matches!(v.get("exec"), Some(Json::Bool(true)));
                tree = if exec {
                    tree.exec(name, &bytes)
                } else {
                    tree.file(name, &bytes)
                };
            }
            "symlink" => {
                let target = v
                    .get("target")
                    .and_then(Json::as_str)
                    .ok_or_else(|| format!("{name}: missing target"))?;
                tree = tree.symlink(name, target);
            }
            "dir" => {
                let sub = v
                    .get("entries")
                    .ok_or_else(|| format!("{name}: missing entries"))?;
                tree = tree.dir(name, manifest_to_tree(sub)?);
            }
            other => return Err(format!("{name}: unknown entry kind {other}")),
        }
    }
    Ok(tree)
}
