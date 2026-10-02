//! AC-R-2.11.4-11 — the per-surface operation inventory, executable.
//!
//! Every surface crosses the kernel↔surfaces boundary only through
//! registered `hh-embed/1` operations (ADR-0179; OQ-130): the CLI and the
//! ACP artefact drive binding (a) `svc.handle` / the generated client's
//! `call`, the web surface the generated client, and the MCP server its
//! exposure table plus `op_call` pass-throughs. An unregistered method is
//! already refused at dispatch (`ops::lookup` fails typed), so a
//! "private verb" is structurally impossible at runtime — this test makes
//! the *compile-time* claim auditable: every method-position string
//! literal in a surface crate's `src/` must name a registry op, and every
//! `format!` op template must expand under a registered namespace.
//!
//! The scanned set is also the data the readiness report's per-surface
//! inventory table asserts (docs/build/reports/S5.8-surface-inventory.md).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use hh_embed_schema::ops;

/// Surface crate → the binding it crosses on (ADR-0179 D2; recorded by the
/// inventory report).
const SURFACES: &[(&str, &str)] = &[
    (
        "hh-cli",
        "in_process (binding a) / generated client over stdio",
    ),
    ("hh-web", "generated client over stdio"),
    (
        "hh-mcp-lab",
        "in_process (binding a) via EmbedService::handle",
    ),
    ("hh-acp", "in_process (binding a) via EmbedService::handle"),
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `true` when `c` can continue a Rust identifier — the position check
/// that keeps `tool_call(`/`methodology` out of the `call(`/`op(` match.
fn ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The text span from `from` up to (not including) the first `)` or `;`
/// — the argument head where a method literal would sit.
fn arg_head(text: &str, from: usize) -> &str {
    let tail = &text[from..];
    let end = tail
        .find([')', ';'])
        .map(|i| i.min(tail.len()))
        .unwrap_or(tail.len());
    &tail[..end]
}

/// The first `"…"` literal inside `span`, if any (string escapes are not
/// needed by the sources scanned — `"\"` never occurs inside an op name).
fn first_literal(span: &str) -> Option<String> {
    let start = span.find('"')? + 1;
    let end = span[start..].find('"')? + start;
    Some(span[start..end].to_string())
}

/// Every `"…"` literal inside `span` (used for the multi-arg `tool(`
/// table rows).
fn literals(span: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = span;
    while let Some(l) = first_literal(rest) {
        let consumed = rest.find('"').unwrap() + l.len() + 2;
        out.push(l);
        rest = &rest[consumed.min(rest.len())..];
    }
    out
}

/// The balanced-`(`…`)` span starting at `open` (the index of `(`), or
/// `None` if unbalanced — needed for the multi-line `tool(` table rows.
fn balanced(text: &str, open: usize) -> Option<&str> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut i = open;
    let mut in_str = false;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if in_str {
            if c == '"' {
                in_str = false;
            }
        } else {
            match c {
                '"' => in_str = true,
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&text[open..=i]);
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

/// Scan one source file for method-position literals:
///   * `call(` / `.call(` / `call(b, "op"` — first literal before `)`;
///   * `method:` — the literal form only (`method: "hello"`);
///   * `.op(` / `op_shared(` / `op_call(` — first literal before `)`;
///   * `tool(` — the MCP exposure-table helper: the op is the third
///     string literal (`name`, `semantic_id`, `op`, `effect`, …);
///   * `format!("ns.{var}")` inside a call — recorded as a template row.
fn scan(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    // Call positions: one literal carrying the op name.
    for pat in ["call(", "method:", ".op(", "op_shared(", "op_call("] {
        let mut at = 0;
        while let Some(i) = text[at..].find(pat) {
            let pos = at + i;
            // `call(`/`op_shared(`/`op_call(` must not be the tail of a
            // longer identifier (`tool_call`, `fn call`); a leading `.`
            // pattern (`.op(`) is already receiver-bounded — `self.op(`
            // is exactly the call position we want.
            let clean =
                pat.starts_with('.') || pos == 0 || !ident(text.as_bytes()[pos - 1] as char);
            let head_start = pos + pat.len();
            if clean {
                let head = arg_head(text, head_start);
                if let Some(l) = first_literal(head) {
                    found.insert(l);
                }
            }
            at = pos + pat.len();
        }
    }
    // `tool(` table rows — the op is the third literal of the balanced
    // call (two names precede it; `""` means a lowering-only tool).
    let mut at = 0;
    while let Some(i) = text[at..].find("tool(") {
        let pos = at + i;
        let clean = pos == 0 || !ident(text.as_bytes()[pos - 1] as char);
        if clean {
            if let Some(span) = balanced(text, pos + 4) {
                let lits = literals(span);
                if lits.len() >= 3 && !lits[2].is_empty() {
                    found.insert(lits[2].clone());
                }
            }
        }
        at = pos + 5;
    }
    found
}

/// `prefix*` templates (`format!("lab.registry.{verb}")`) — the literal
/// part before the first `{`.
fn template_prefix(lit: &str) -> Option<&str> {
    lit.find('{').map(|i| &lit[..i])
}

#[test]
fn every_surface_method_literal_names_a_registry_op() {
    let ops: BTreeSet<String> = ops::registry().iter().map(|o| o.name.to_string()).collect();
    let root = root();
    let mut inventory: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    let mut templates: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for (surface, _binding) in SURFACES {
        let mut used = BTreeSet::new();
        let mut tmpl = BTreeSet::new();
        let dir = root.join("crates").join(surface).join("src");
        for ent in walk(&dir) {
            let text = std::fs::read_to_string(&ent)
                .unwrap_or_else(|e| panic!("read {}: {e}", ent.display()));
            for lit in scan(&text) {
                if let Some(prefix) = template_prefix(&lit) {
                    assert!(
                        ops.iter().any(|o| o.starts_with(prefix)),
                        "{surface}: op template `{lit}` expands under no registered namespace",
                    );
                    tmpl.insert(lit);
                } else {
                    assert!(
                        ops.contains(&lit),
                        "{surface}: method-position literal `{lit}` in {} \
                         names no registered op — surfaces may not carry \
                         private verbs (AC-R-2.11.4-11)",
                        ent.display(),
                    );
                    used.insert(lit);
                }
            }
        }
        assert!(
            !used.is_empty(),
            "{surface}: scanner found no op literals — the inventory check is vacuous"
        );
        inventory.insert(surface, used);
        templates.insert(surface, tmpl);
    }
    // The report's table is generated from this output — print it so the
    // run record can diff inventories across tickets.
    for (surface, used) in &inventory {
        eprintln!("── {surface} ({} ops)", used.len());
        let reg = ops::registry();
        for op in used {
            let spec = reg.iter().find(|o| o.name == op).unwrap();
            eprintln!("   {op}  [{}]", spec.tier.as_str());
        }
        for t in &templates[surface] {
            eprintln!("   {t}  [template family]");
        }
    }
}

/// Recursive `*.rs` walker.
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for ent in std::fs::read_dir(dir).unwrap() {
        let p = ent.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
    out
}
