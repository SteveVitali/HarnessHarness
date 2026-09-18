//! The module-graph lint (spec §8.4 §3 "module-graph lint"; ADR-0180 D2's
//! physical half; R-2.12.2 "the module-graph lint"). For every
//! **first-party in-process** variant — the only placement whose code shares
//! the kernel's crate graph — the lint walks `use`/`mod` edges and refuses a
//! variant whose reachable set contains a module it may not name (X1's
//! in-crate form):
//!
//! - another variant's module (`variants/` is not a dependency surface — a
//!   first-party variant is a *plugin-like* component and may not reach
//!   sibling internals);
//! - the kernel's projection/handle internals (`hh-kernel::proj`, handles —
//!   only the *granted views* are reachable, never the internals);
//! - anything outside the kernel crate set at all (a variant imports kernel
//!   contracts — `hh_hir`, `hh_registry`, `hh_ontology`, … — plus `std`, never
//!   third-party code: the dependency policy pins `default-run` = no external
//!   crates, and a variant cannot relax it).
//!
//! Stage-1 scope (ADR-0180 D2/D5, T-LCD-06): the check runs over *source* —
//! `use hh_foo::…` edges — because first-party in-process code is Rust in this
//! workspace. Out-of-process placements (subprocess/container/remote — the
//! non-first-party set) are isolated by `Placement` at registration and by
//! `locality_unsupported` at Stage 1; the lint does not apply to them.
//!
//! The lint is a pure function over the workspace layout —
//! `lint_workspace(root)` walks `crates/*/src`, collects the in-process variant
//! modules (any `crates/hh-*/src/variants/**`), and reports violations. A
//! caller may also lint a single module's source text via
//! [`lint_module_source`] — the admission path uses it on registered
//! first-party variants (AC-9; an admitted variant is lint-clean).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// The kernel crates a first-party in-process variant may name (the contract
/// surface — a variant reaches *contracts*, never internals).
const ALLOWED_CRATES: &[&str] = &[
    "hh_wire",
    "hh_identity",
    "hh_provenance",
    "hh_ontology",
    "hh_hir",
    "hh_registry",
    "hh_assembly",
    "hh_embed_schema",
    "hh_monitor",
    "hh_control",
    "hh_embed",
    "hh_experiment",
    "hh_plugin",
    "hh_lab",
    "hh_codegen",
    "hh_lifecycle",
    "hh_determinism",
    "hh_conformance",
];

/// Root modules that are always allowed (`std`, `core`, `alloc`, `crate`,
/// `self`, `super`).
const ALLOWED_ROOTS: &[&str] = &["std", "core", "alloc", "crate", "self", "super"];

/// One lint violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintViolation {
    /// The offending file (`crates/…`-relative when linting a workspace).
    pub file: String,
    /// The line number (1-based) when known.
    pub line: usize,
    /// The offending edge (`use hh_registry::store::…`, `mod helper`, …).
    pub edge: String,
    /// The rule it broke.
    pub rule: LintRule,
}

/// The lint rules (each a physical consequence of X1/X3 for in-process code).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintRule {
    /// A variant reached a sibling variant's module (`variants/` is not a
    /// dependency surface — cross-variant reuse goes through a *contract*).
    SiblingVariantReach,
    /// A variant reached kernel internals — a `use` naming a module the
    /// contract surface does not export to variants (`store` internals are
    /// reached *through* `Registry`, never via private paths). Stage-1 form:
    /// any `use` whose root is not in the allowed set is an internals/unknown
    /// reach.
    InternalReach,
    /// A variant's `mod` declared a child module — the module graph is the
    /// crate's, not the variant's (a variant contributes *implementations*;
    /// it does not sprout kernel submodules — X3 keeps interaction mediated).
    SubmoduleDeclaration,
    /// A variant used `unsafe` — the in-process surface is safe Rust only
    /// (in-process variants run in the kernel's address space; `unsafe` would
    /// import the kernel's memory safety on the variant's authority — C-TRUST).
    UnsafeUse,
    /// The file could not be read/parsed.
    Unreadable,
}

/// The lint report.
#[derive(Debug, Default)]
pub struct LintReport {
    /// Every violation, in file order.
    pub violations: Vec<LintViolation>,
    /// Files scanned.
    pub files: usize,
    /// Variant modules found (`crates/*/src/variants/**`).
    pub variant_modules: usize,
}

impl LintReport {
    /// Whether the module graph is clean.
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }
}

/// Lint a whole workspace: every `*.rs` under `crates/*/src/variants/**` is a
/// first-party in-process variant module and is checked. Returns the report.
/// An absent `crates/` lints clean (an empty variant set has no reach).
pub fn lint_workspace(root: &Path) -> LintReport {
    let mut report = LintReport::default();
    let crates = root.join("crates");
    let Ok(rd) = fs::read_dir(&crates) else {
        return report;
    };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for crate_dir in entries {
        let src = crate_dir.join("src").join("variants");
        if !src.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        collect_rs(&src, &mut files);
        for f in files {
            report.variant_modules += 1;
            lint_file(&f, root, &mut report);
        }
    }
    report
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for e in entries {
        if e.is_dir() {
            collect_rs(&e, out);
        } else if e.extension().map(|x| x == "rs").unwrap_or(false) {
            out.push(e);
        }
    }
}

/// Lint one source file against the rules, appending to `report`.
fn lint_file(path: &Path, root: &Path, report: &mut LintReport) {
    report.files += 1;
    let rel = path
        .strip_prefix(root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string());
    let Ok(text) = fs::read_to_string(path) else {
        report.violations.push(LintViolation {
            file: rel,
            line: 0,
            edge: String::new(),
            rule: LintRule::Unreadable,
        });
        return;
    };
    for v in lint_module_source(&text) {
        report.violations.push(LintViolation {
            file: rel.clone(),
            line: v.line,
            edge: v.edge,
            rule: v.rule,
        });
    }
}

/// Lint one module's *source text* — the form the admission path uses on a
/// first-party in-process variant before registration (AC-9). Returns the
/// violations (empty = clean).
///
/// Rules, line-oriented (the kernel style is one `use` per line — a `use`
/// group with a `{a, b}` list is checked per root segment, which is what the
/// rule needs; we do not parse Rust):
/// - `use <root>::…` — `<root>` must be in `ALLOWED_ROOTS ∪ ALLOWED_CRATES`;
///   anything else is an internals/unknown reach (InternalReach).
/// - `use …::variants::…` / `use crate::variants::…` from inside a variant —
///   sibling reach (SiblingVariantReach). (`crate::` is allowed *except* when
///   the path re-enters `variants` — a variant may not name siblings even
///   relatively.)
/// - `unsafe` as a token — UnsafeUse.
/// - `mod <name>;` — SubmoduleDeclaration.
pub fn lint_module_source(source: &str) -> Vec<LintViolation> {
    let mut out = Vec::new();
    for (i, raw) in source.lines().enumerate() {
        let line = raw.trim();
        let ln = i + 1;
        if line.starts_with("//") || line.is_empty() {
            continue;
        }
        if let Some(rest) = strip_use(line) {
            let root = rest
                .split([':', ' ', '{'])
                .next()
                .unwrap_or("");
            if !ALLOWED_ROOTS.contains(&root) && !ALLOWED_CRATES.contains(&root) {
                out.push(LintViolation {
                    file: String::new(),
                    line: ln,
                    edge: line.to_string(),
                    rule: LintRule::InternalReach,
                });
            }
            // sibling reach — `…::variants::…` anywhere in the path.
            if rest.split("::").any(|seg| seg.trim() == "variants") {
                out.push(LintViolation {
                    file: String::new(),
                    line: ln,
                    edge: line.to_string(),
                    rule: LintRule::SiblingVariantReach,
                });
            }
        }
        // `mod name;` (not `mod name {` — an inline module body inside a
        // variant file is a submodule too, but the `;` form is what the lint
        // guards; a `{` body is caught by the same rule).
        if is_mod_decl(line) {
            out.push(LintViolation {
                file: String::new(),
                line: ln,
                edge: line.to_string(),
                rule: LintRule::SubmoduleDeclaration,
            });
        }
        if contains_unsafe(line) {
            out.push(LintViolation {
                file: String::new(),
                line: ln,
                edge: line.to_string(),
                rule: LintRule::UnsafeUse,
            });
        }
    }
    out
}

/// Strip a visibility prefix (`pub`, `pub(crate)`, `pub(super)`, `pub(in …)`).
fn strip_vis(line: &str) -> &str {
    let l = line.trim_start();
    if let Some(rest) = l.strip_prefix("pub") {
        let rest = rest.trim_start();
        if let Some(r) = rest.strip_prefix('(') {
            if let Some(end) = r.find(')') {
                return r[end + 1..].trim_start();
            }
            return l;
        }
        return rest;
    }
    l
}

/// `use path;`/`pub use path;` → the path (leading whitespace/comments
/// stripped). `None` when the line is not a use.
fn strip_use(line: &str) -> Option<&str> {
    let l = strip_vis(line);
    let l = l.strip_prefix("use ")?;
    Some(l.trim_end_matches(';').trim())
}

/// `mod name;` or `mod name {` — a module declaration.
fn is_mod_decl(line: &str) -> bool {
    let l = strip_vis(line);
    let Some(rest) = l.strip_prefix("mod ") else {
        return false;
    };
    let rest = rest.trim_end();
    rest.ends_with(';') || rest.ends_with('{')
}

/// `unsafe` as a token (not `unsafe_code` — a word boundary both sides).
fn contains_unsafe(line: &str) -> bool {
    let b = line.as_bytes();
    let mut i = 0;
    while let Some(off) = line[i..].find("unsafe") {
        let start = i + off;
        let end = start + 6;
        let ok_start = start == 0 || !b[start - 1].is_ascii_alphanumeric() && b[start - 1] != b'_';
        let ok_end = end >= b.len() || !b[end].is_ascii_alphanumeric() && b[end] != b'_';
        if ok_start && ok_end {
            return true;
        }
        i = end;
    }
    false
}

/// The set of files the lint scanned, for callers that report evidence.
pub fn scanned_files(report: &LintReport) -> BTreeSet<String> {
    report.violations.iter().map(|v| v.file.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_variant_source_lints_clean() {
        let src = "\
//! A variant.
use hh_registry::registry::Registry;
use hh_wire::json::Json;
use std::collections::BTreeMap;

pub fn run() { let _ = Json::Null; }
";
        assert!(lint_module_source(src).is_empty());
    }

    #[test]
    fn sibling_variant_reach_is_a_violation() {
        let src = "use crate::variants::other::Helper;\n";
        let v = lint_module_source(src);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, LintRule::SiblingVariantReach);
    }

    #[test]
    fn unknown_root_is_internal_reach() {
        let src = "use serde_json::Value;\n";
        let v = lint_module_source(src);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, LintRule::InternalReach);
    }

    #[test]
    fn kernel_internal_path_is_internal_reach() {
        // `hh_kernel` is not in the allowed set — internals are reached
        // through granted views, never by path (X3).
        let src = "use hh_kernel::proj::Projection;\n";
        let v = lint_module_source(src);
        assert_eq!(v[0].rule, LintRule::InternalReach);
    }

    #[test]
    fn unsafe_is_a_violation() {
        let src = "pub unsafe fn p() {}\nlet x = unsafe { 0 };\n// unsafe comment\n";
        let v = lint_module_source(src);
        assert_eq!(v.len(), 2); // comment line skipped
        assert!(v.iter().all(|x| x.rule == LintRule::UnsafeUse));
    }

    #[test]
    fn mod_decl_is_a_violation() {
        let src = "mod helper;\npub(crate) mod inner {\n}\n";
        let v = lint_module_source(src);
        assert_eq!(v.len(), 2);
        assert!(v.iter().all(|x| x.rule == LintRule::SubmoduleDeclaration));
    }

    #[test]
    fn workspace_lint_runs() {
        // This workspace's own crates — no `src/variants/` dirs exist yet, so
        // the lint is vacuously clean but exercises the walker.
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let r = lint_workspace(&root);
        assert!(r.is_clean(), "{:?}", r.violations);
    }
}
