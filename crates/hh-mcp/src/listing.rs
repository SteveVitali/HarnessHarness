//! `import_listing`'s wire half (§5d.1 §2 `import_listing` /
//! §5d.4 R-2.5.4; ticket S3.9): the `hh-mcp-listing/1` document the
//! registry consumes — a *verbatim* tool array plus per-tool and
//! whole-listing content hashes. Nothing is interpreted here: a wire
//! `Tool` is opaque data at this layer, unknown `_meta` keys ride in
//! verbatim (N4), and `listing_hash` is the `pin` attestation's subject
//! (ADR-0088 D3 — a pin over `listing_hash` never transfers).
//!
//! `listing_delta` is `refresh`'s detection half — the registry op
//! decides what to do with it (`supersedes{reason: edit}`, lineage,
//! classification); this layer only reports *what* changed.

use hh_wire::json::Json;

/// The lifted-listing document schema id.
pub const LISTING_SCHEMA: &str = "hh-mcp-listing/1";

/// `import_listing(server_ref, tools) → listing_doc` — the pinned
/// document: `{schema, server_ref, listing_hash, tools: [{name,
/// listing_hash, tool}]}`. `tool` is the wire `Tool` **verbatim** —
/// the registry's lift reads `description`/`inputSchema`/`annotations`/
/// `_meta` off it; unknown keys survive (N4).
pub fn import_listing(server_ref: &str, tools: &[Json]) -> Json {
    let entries: Vec<Json> = tools
        .iter()
        .map(|t| {
            Json::obj([
                ("name", t.get("name").cloned().unwrap_or(Json::Null)),
                ("listing_hash", Json::str(tool_listing_hash(t))),
                ("tool", t.clone()),
            ])
        })
        .collect();
    Json::obj([
        ("schema", Json::str(LISTING_SCHEMA)),
        ("server_ref", Json::str(server_ref.to_string())),
        ("listing_hash", Json::str(listing_hash(tools))),
        ("tools", Json::Arr(entries)),
    ])
}

/// `idp` over one wire `Tool`'s canonical bytes — the per-surface
/// `pin`/`refresh` subject.
pub fn tool_listing_hash(tool: &Json) -> String {
    hh_identity::idp_id("mcp.listing.tool", tool.to_canonical_string().as_bytes())
}

/// `idp` over the canonical tool array — the whole-listing hash the
/// `refresh` no-op rule compares (unchanged hash ⇒ no-op).
pub fn listing_hash(tools: &[Json]) -> String {
    let doc = Json::Arr(tools.to_vec());
    hh_identity::idp_id("mcp.listing", doc.to_canonical_string().as_bytes())
}

/// Read a `hh-mcp-listing/1` document back to `(server_ref, tools)` —
/// the registry's entry point. Malformed documents are `Err`, never
/// silently read.
pub fn listing_doc(doc: &Json) -> Result<(String, Vec<Json>), String> {
    let schema = doc.get("schema").and_then(Json::as_str).unwrap_or("");
    if schema != LISTING_SCHEMA {
        return Err(format!("schema `{schema}` — expected {LISTING_SCHEMA}"));
    }
    let server_ref = doc
        .get("server_ref")
        .and_then(Json::as_str)
        .ok_or("server_ref missing")?
        .to_string();
    let tools = match doc.get("tools") {
        Some(Json::Arr(ts)) => ts
            .iter()
            .map(|e| e.get("tool").cloned().unwrap_or(Json::Null))
            .collect(),
        _ => return Err("tools[] missing".to_string()),
    };
    Ok((server_ref, tools))
}

/// `refresh`'s detection half — name-keyed delta between two listing
/// docs (`removed` surfaces stay addressable by identity; the registry
/// marks them `unavailable`).
#[derive(Debug, Clone, PartialEq)]
pub struct ListingDelta {
    /// Names new in `new`.
    pub added: Vec<String>,
    /// Names absent from `new`.
    pub removed: Vec<String>,
    /// Names present in both whose `listing_hash` differs.
    pub changed: Vec<String>,
    /// `new == prev` byte-for-byte (the no-op rule).
    pub unchanged: bool,
}

/// `listing_delta(prev_doc, new_doc)` — compare the `hh-mcp-listing/1`
/// documents' tool sets by `listing_hash`.
pub fn listing_delta(prev: &Json, new: &Json) -> Result<ListingDelta, String> {
    let (prev_ref, prev_tools) = listing_doc(prev)?;
    let (new_ref, new_tools) = listing_doc(new)?;
    if prev_ref != new_ref {
        return Err(format!(
            "server_ref drift: `{prev_ref}` vs `{new_ref}` — refresh is per-source"
        ));
    }
    let hashes = |tools: &[Json]| -> std::collections::BTreeMap<String, String> {
        tools
            .iter()
            .map(|t| {
                (
                    t.get("name")
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string(),
                    tool_listing_hash(t),
                )
            })
            .collect()
    };
    let p = hashes(&prev_tools);
    let n = hashes(&new_tools);
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    for (name, h) in &n {
        match p.get(name) {
            None => added.push(name.clone()),
            Some(old) if old != h => changed.push(name.clone()),
            _ => {}
        }
    }
    for name in p.keys() {
        if !n.contains_key(name) {
            removed.push(name.clone());
        }
    }
    added.sort();
    removed.sort();
    changed.sort();
    Ok(ListingDelta {
        added,
        removed,
        changed,
        unchanged: prev == new,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str, desc: &str) -> Json {
        Json::obj([
            ("name", Json::str(name)),
            ("description", Json::str(desc)),
            ("inputSchema", Json::obj([("type", Json::str("object"))])),
        ])
    }

    #[test]
    fn listing_doc_round_trip_and_hash() {
        let tools = vec![tool("a", "1"), tool("b", "2")];
        let doc = import_listing("srv:1", &tools);
        let (sref, back) = listing_doc(&doc).unwrap();
        assert_eq!(sref, "srv:1");
        assert_eq!(back, tools);
        // Hash is stable and sensitive.
        assert_eq!(
            doc.get("listing_hash").and_then(Json::as_str),
            Some(listing_hash(&tools).as_str())
        );
        let other = import_listing("srv:1", &[tool("a", "1"), tool("b", "3")]);
        assert_ne!(
            doc.get("listing_hash"),
            other.get("listing_hash"),
            "content change ⇒ hash change"
        );
    }

    #[test]
    fn delta_is_name_keyed() {
        let a = import_listing("s", &[tool("x", "1"), tool("y", "2")]);
        let b = import_listing("s", &[tool("y", "3"), tool("z", "9")]);
        let d = listing_delta(&a, &b).unwrap();
        assert_eq!(d.added, vec!["z"]);
        assert_eq!(d.removed, vec!["x"]);
        assert_eq!(d.changed, vec!["y"]);
        assert!(!d.unchanged);
        let d_same = listing_delta(&a, &a).unwrap();
        assert!(d_same.unchanged && d_same.changed.is_empty());
    }
}
