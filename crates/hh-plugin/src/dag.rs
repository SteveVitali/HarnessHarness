//! The spec-DAG check (spec §8.4 §2 `spec_dag_check(sections, class_records,
//! plugins) → {tier_violations[], cycles[]}`; ADR-0182 X2; the readiness gate —
//! "the checker parses the rows, so a row edit that breaks a rule is caught at
//! the next run"; the generalisation of T-LCD-06's `hosting_edges = []`).
//!
//! `sections` is the spec text — the §4.4 dependency tables of
//! `spec/CANONICAL_SPEC.md` parsed **as data**: every item row declares a node
//! at its tier; `` `R-x.y.zⁿ…` `` slice nodes take the superscript digit's tier
//! (`⁰` → C0, `¹` → C1, …); the Core column contributes `item → target` /
//! `` `slice` → target` `` edges (bare lists are `item → …`); the Extension
//! column contributes `item → target (Cn)` edges where the annotation is the
//! *defining tier* of the consumed contract. `KS` is the C0 kernel-substrate
//! node. A referenced node that is never declared and never tier-annotated is a
//! **parse error** — a dangling dependency is caught, never silently ignored
//! (CC3).
//!
//! `class_records` and `plugins` extend the same graph: a `ClassRecord` is a
//! node `class:<class_id>` at its declared `tier` with an edge per
//! `depends_on` [`ContractRef`] landing on `contract:<kind>:<id>` at the
//! contract's *defining* tier ([`crate::contract::contract_defining_tier`]); a
//! plugin manifest is a node `plugin:<ns>/<name>` at its `tier` with edges per
//! `depends_on` + `requires`. A `ContractRef` whose defining tier is unknown is
//! reported in `unknown_refs` (X1 makes it the only dependency form — an
//! unnameable dependency is itself a finding).
//!
//! The graph machinery (nodes/edges, tier monotonicity, acyclicity) is
//! `hh_ontology::dag`'s — the one engine (CC7); this module supplies the §4.4
//! data plane. `DagReport.tier_violations`/`cycles` are the AC-8 surface.

use std::collections::BTreeMap;

use hh_ontology::dag::{check, DagReport, Edge, NodeKind, SpecDag};

use crate::contract::{contract_defining_tier, ClassContract, ContractRef};

/// The kernel-substrate node (tier C0 — every item may depend on it).
pub const KS_NODE: &str = "KS";

/// A class record as the DAG check consumes it (`ClassRecord` → this view —
/// the check needs `class_id`, `tier`, `depends_on` only).
#[derive(Debug, Clone, PartialEq)]
pub struct ClassDagInput {
    /// The class id.
    pub class_id: String,
    /// The declared tier (`ClassRecord.tier` — the contract's defining tier).
    pub tier: u8,
    /// The class's `depends_on` contract refs.
    pub depends_on: Vec<ContractRef>,
}

/// A plugin manifest as the DAG check consumes it (`PluginManifest` → this
/// view — `dag_input()` on the manifest).
#[derive(Debug, Clone, PartialEq)]
pub struct PluginDagInput {
    /// The plugin's `<namespace>/<name>` identity.
    pub plugin_id: String,
    /// The manifest's declared `tier` (`C0|C1|C2|C4`).
    pub tier: u8,
    /// The manifest's `depends_on` plus every `requires` contract (a
    /// `requires` is a dependency — X1 makes it the only reach).
    pub depends_on: Vec<ContractRef>,
}

/// The parse outcome + the built graph — every parse problem is surfaced as
/// data (a malformed row is a gate failure, never a silent skip).
#[derive(Debug)]
pub struct ParsedSpecDag {
    /// The graph (nodes + edges collected across both tables).
    pub dag: SpecDag,
    /// Rows consumed (the item table + the slice table).
    pub rows: usize,
    /// Edges to `KS` (the verification statement counts them).
    pub ks_edges: usize,
    /// Item nodes declared (the tiered items).
    pub items: usize,
    /// Slice nodes declared (`R-…ⁿ…` — both tables).
    pub slices: usize,
    /// Contract nodes materialised from `depends_on` refs.
    pub contract_nodes: usize,
    /// Parse problems (malformed rows, dangling references).
    pub errors: Vec<String>,
    /// Declared node id → tier (populated as the tables parse — a dependency
    /// naming a never-declared node is a parse error).
    known: BTreeMap<String, u8>,
}

/// The full check report (the AC-8 surface — `tier_violations` and `cycles`
/// must be empty; `unknown_refs` and `parse_errors` must be empty too — an
/// unnameable or undeclared dependency is a finding, not a pass).
#[derive(Debug)]
pub struct SpecDagReport {
    /// The engine report.
    pub inner: DagReport,
    /// `depends_on`/`requires` refs with no known defining tier.
    pub unknown_refs: Vec<String>,
    /// Spec-table parse problems.
    pub parse_errors: Vec<String>,
    /// Node/edge counts (the verification statement's numbers).
    pub nodes: usize,
    /// Total edges.
    pub edges: usize,
    /// Edges to `KS`.
    pub ks_edges: usize,
}

impl SpecDagReport {
    /// Whether the DAG is clean: no tier violation, no cycle, no unnameable
    /// dependency, no parse problem. (`hosting_edges`/`proj_abi_total` are the
    /// §2-level properties the ontology's own graph checks — the §4.4 graph has
    /// no `Hir`-kind nodes, so both hold vacuously.)
    pub fn is_clean(&self) -> bool {
        self.inner.tier_violations.is_empty()
            && self.inner.cycles.is_empty()
            && self.unknown_refs.is_empty()
            && self.parse_errors.is_empty()
    }

    /// The AC-8 pair.
    pub fn tier_violations(&self) -> &[Edge] {
        &self.inner.tier_violations
    }
    /// The AC-8 pair.
    pub fn cycles(&self) -> &[String] {
        &self.inner.cycles
    }
}

/// `spec_dag_check(sections, class_records, plugins) → report` (§8.4 §2).
/// `sections` is the spec text (the §4.4 tables are parsed out of it).
pub fn spec_dag_check(
    sections: &str,
    class_records: &[ClassDagInput],
    plugins: &[PluginDagInput],
) -> SpecDagReport {
    let mut parsed = parse_spec_tables(sections);
    let mut unknown_refs: Vec<String> = Vec::new();
    let classes: Vec<ClassContract> = class_records
        .iter()
        .map(|c| ClassContract {
            class_id: c.class_id.clone(),
            contract_version: String::new(),
            tier: c.tier,
            decision_points: Vec::new(),
        })
        .collect();
    let materialise = |parsed: &mut ParsedSpecDag,
                       owner: &str,
                       owner_tier: u8,
                       refs: &[ContractRef],
                       unknown: &mut Vec<String>| {
        parsed.dag.add_node(owner, owner_tier, NodeKind::Instrument);
        for r in refs {
            let target = format!("contract:{}", r.label());
            match contract_defining_tier(r, &classes) {
                Some(t) => {
                    if !parsed.known.contains_key(&target) {
                        parsed.dag.add_node(&target, t, NodeKind::Instrument);
                        parsed.known.insert(target.clone(), t);
                        parsed.contract_nodes += 1;
                    }
                    parsed.dag.add_edge(owner, &target);
                }
                None => unknown.push(format!("{owner} → {}", r.label())),
            }
        }
    };
    for c in class_records {
        let owner = format!("class:{}", c.class_id);
        materialise(
            &mut parsed,
            &owner,
            c.tier,
            &c.depends_on,
            &mut unknown_refs,
        );
    }
    for p in plugins {
        let owner = format!("plugin:{}", p.plugin_id);
        materialise(
            &mut parsed,
            &owner,
            p.tier,
            &p.depends_on,
            &mut unknown_refs,
        );
    }
    unknown_refs.sort();
    unknown_refs.dedup();
    let nodes = parsed.dag.node_count();
    let edges = parsed.dag.edge_count();
    SpecDagReport {
        inner: check(&parsed.dag),
        unknown_refs,
        parse_errors: parsed.errors.clone(),
        nodes,
        edges,
        ks_edges: parsed.ks_edges,
    }
}

// ── §4.4 table parsing ────────────────────────────────────────────────────────

/// The superscript-digit → tier map for slice ids (`⁰` → C0 … `⁴` → C4).
fn superscript_tier(id: &str) -> Option<u8> {
    for c in id.chars() {
        match c {
            '⁰' => return Some(0),
            '¹' => return Some(1),
            '²' => return Some(2),
            '³' => return Some(3),
            '⁴' => return Some(4),
            _ => {}
        }
    }
    None
}

/// Extract the base item id (`R-x.y.z`, optional trailing letter like
/// `R-2.7.2a`) — `None` when `s` does not start with one.
fn item_id(s: &str) -> Option<&str> {
    let tail = s.strip_prefix("R-")?;
    let bytes = tail.as_bytes();
    let mut i = 0;
    // digits '.' digits '.' digits [a-z]
    let mut dots = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_digit() {
            i += 1;
        } else if b == b'.' && dots < 2 {
            dots += 1;
            i += 1;
        } else {
            break;
        }
    }
    if dots != 2 || i == 0 {
        return None;
    }
    if i < bytes.len() && bytes[i].is_ascii_alphabetic() {
        // item ids like `R-2.7.2a` — a single trailing ascii letter.
        i += 1;
    }
    Some(&s[..i + 2])
}

/// Extract every `R-x.y.z[sup]` / `KS` token from a cell, in order.
fn ref_tokens(cell: &str) -> Vec<String> {
    let mut out = Vec::new();
    let chars: Vec<char> = cell.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == 'K'
            && i + 1 < chars.len()
            && chars[i + 1] == 'S'
            && (i == 0 || !chars[i - 1].is_alphanumeric())
            && (i + 2 >= chars.len() || !chars[i + 2].is_alphanumeric())
        {
            out.push(KS_NODE.to_string());
            i += 2;
            continue;
        }
        if chars[i] == 'R'
            && i + 1 < chars.len()
            && chars[i + 1] == '-'
            && (i == 0 || !chars[i - 1].is_alphanumeric())
        {
            let rest: String = chars[i..].iter().collect();
            if let Some(base) = item_id(&rest) {
                let mut id = base.to_string();
                let mut j = base.chars().count();
                // consume superscript modifiers (⁰¹²³⁴ᵃᵇᶜᵈᵉᶠ)
                while j < rest.chars().count() {
                    let c = rest.chars().nth(j).unwrap();
                    if "⁰¹²³⁴ᵃᵇᶜᵈᵉᶠ".contains(c) {
                        id.push(c);
                        j += 1;
                    } else {
                        break;
                    }
                }
                out.push(id.clone());
                i += id.chars().count();
                continue;
            }
        }
        i += 1;
    }
    out
}

/// The tier annotation `(Cn)` in an extension-column entry, when present.
fn ext_tier(cell_entry: &str) -> Option<u8> {
    let l = cell_entry.find('(')?;
    let r = cell_entry.find(')')?;
    let inner = &cell_entry[l + 1..r];
    inner.trim().strip_prefix('C')?.parse::<u8>().ok()
}

/// Parse the two §4.4 tables out of `spec_text`.
///
/// - Table 1 (items): `| `R-x.y.z` | §… | Cn · Sm | C0 slices | core deps |
///   ext deps | ADRs |` — 7 cells.
/// - Table 2 (slices at another tier): `| `R-x.y.zⁿ…` | `R-x.y.z` (Ck) | Cn · Sm
///   | content | deps | ADRs |` — 6 cells, first cell carries a superscript.
///
/// The pass order matters: all node declarations are collected before edges
/// resolve, so a forward reference to a slice node declared later is fine.
pub fn parse_spec_tables(spec_text: &str) -> ParsedSpecDag {
    let mut parsed = ParsedSpecDag {
        dag: SpecDag::new(),
        rows: 0,
        ks_edges: 0,
        items: 0,
        slices: 0,
        contract_nodes: 0,
        errors: Vec::new(),
        known: BTreeMap::new(),
    };
    parsed.known.insert(KS_NODE.to_string(), 0);
    parsed.dag.add_node(KS_NODE, 0, NodeKind::Instrument);

    // ── Pass 1: declare every node.
    for (ln, line) in spec_text.lines().enumerate() {
        let cells = table_cells(line);
        match cells.len() {
            7 => {
                // Item row — first cell `R-x.y.z` (no superscript).
                let id = cells[0].trim().trim_matches('`');
                let Some(base) = item_id(id) else {
                    continue;
                };
                if id != base || superscript_tier(id).is_some() {
                    continue;
                }
                let tier = cell_tier(&cells[2], ln);
                let Some(tier) = tier else {
                    // `—` tier = a program-level row (R-2.12.3/4/5 are program
                    // decisions, not tiered items — the 62-item count excludes
                    // them). Not a node; not an error.
                    if cells[2].trim().starts_with('—') {
                        continue;
                    }
                    parsed.errors.push(format!(
                        "line {}: item `{id}` has no `Cn` tier cell",
                        ln + 1
                    ));
                    continue;
                };
                declare(&mut parsed, base, tier, ln);
                parsed.items += 1;
                parsed.rows += 1;
                // C0 slice declarations in cell 4 (backticked `R-…⁰…` ids only
                // — a `R-…¹`-or-higher id in this column is a *mention* ("its
                // kernel slice `R-2.6.3¹` is C1, second table"), not a C0
                // declaration; the second table declares those).
                for t in ref_tokens(&cells[3]) {
                    if t != KS_NODE && superscript_tier(&t) == Some(0) {
                        declare(&mut parsed, &t, 0, ln);
                        parsed.slices += 1;
                    }
                }
            }
            6 => {
                // Slice row — first cell carries a superscript tier.
                let id = cells[0].trim().trim_matches('`');
                if item_id(id).is_none() || superscript_tier(id).is_none() {
                    continue;
                }
                let Some(tier) = superscript_tier(id) else {
                    continue;
                };
                // Cross-check against the declared `Cn` cell.
                if let Some(t) = cell_tier(&cells[2], ln) {
                    if t != tier {
                        parsed.errors.push(format!(
                            "line {}: slice `{id}` superscript tier C{tier} ≠ cell tier C{t}",
                            ln + 1
                        ));
                    }
                }
                // The parent cell `` `R-x.y.z` (Cn) `` declares/annotates the
                // parent item — the item may be declared only here when the
                // item row itself is not in the slice's table window.
                if let Some(parent) = ref_tokens(&cells[1]).into_iter().next() {
                    if let Some(pt) = ext_tier(&cells[1]) {
                        declare(&mut parsed, &parent, pt, ln);
                    }
                }
                declare(&mut parsed, id, tier, ln);
                parsed.slices += 1;
                parsed.rows += 1;
            }
            _ => continue,
        }
    }

    // ── Pass 2: edges.
    for (ln, line) in spec_text.lines().enumerate() {
        let cells = table_cells(line);
        match cells.len() {
            7 => {
                let id = cells[0].trim().trim_matches('`');
                let Some(base) = item_id(id) else {
                    continue;
                };
                if id != base || superscript_tier(id).is_some() {
                    continue;
                }
                let item = base.to_string();
                if !parsed.known.contains_key(&item) {
                    continue; // already reported in pass 1
                }
                // Core column — `src → t1, t2` segments (`;`-separated); a bare
                // list is `item → …`.
                for seg in cells[4].split(';') {
                    let seg = seg.trim();
                    if seg.is_empty() || seg == "—" {
                        continue;
                    }
                    let (src, tgts) = match seg.split_once('→') {
                        Some((s, t)) => {
                            let s = s.trim();
                            let src = if s.starts_with("item") {
                                item.clone()
                            } else {
                                let id = s.trim_matches('`').trim();
                                match ref_tokens(id).into_iter().next() {
                                    Some(t) => t,
                                    None => {
                                        parsed.errors.push(format!(
                                            "line {}: dep source `{s}` is not a node",
                                            ln + 1
                                        ));
                                        continue;
                                    }
                                }
                            };
                            (src, t)
                        }
                        None => (item.clone(), seg),
                    };
                    for t in ref_tokens(tgts) {
                        edge(&mut parsed, &src, &t, ln);
                    }
                }
                // Extension column — `X (Cn)` entries (`;` splits `consumed at`
                // notes), source = item.
                for entry in cells[5].split(',') {
                    let entry = entry.trim();
                    if entry.is_empty() || entry == "—" {
                        continue;
                    }
                    let mut toks = ref_tokens(entry).into_iter();
                    let Some(t) = toks.next() else {
                        // `consumed at SN` tails and prose — no ref token.
                        continue;
                    };
                    if let Some(tier) = ext_tier(entry) {
                        if !parsed.known.contains_key(&t) {
                            declare(&mut parsed, &t, tier, ln);
                        } else if parsed.known[&t] != tier {
                            parsed.errors.push(format!(
                                "line {}: `{t}` annotated C{tier} but declared C{}",
                                ln + 1,
                                parsed.known[&t]
                            ));
                        }
                    }
                    edge(&mut parsed, &item, &t, ln);
                }
            }
            6 => {
                let id = cells[0].trim().trim_matches('`');
                if item_id(id).is_none() || superscript_tier(id).is_none() {
                    continue;
                }
                for seg in cells[4].split(';') {
                    let seg = seg.trim();
                    if seg.is_empty() || seg == "—" {
                        continue;
                    }
                    let (src, tgts) = match seg.split_once('→') {
                        Some((s, t)) => {
                            let s = s.trim();
                            let src = if s.starts_with("item") {
                                // the parent item
                                ref_tokens(&cells[1])
                                    .into_iter()
                                    .next()
                                    .unwrap_or_else(|| id.to_string())
                            } else {
                                match ref_tokens(s).into_iter().next() {
                                    Some(t) => t,
                                    None => {
                                        parsed.errors.push(format!(
                                            "line {}: dep source `{s}` is not a node",
                                            ln + 1
                                        ));
                                        continue;
                                    }
                                }
                            };
                            (src, t)
                        }
                        None => (id.to_string(), seg),
                    };
                    for t in ref_tokens(tgts) {
                        edge(&mut parsed, &src, &t, ln);
                    }
                }
            }
            _ => continue,
        }
    }
    parsed
}

/// Split a table row into its cells (empty when the line is not a table row).
fn table_cells(line: &str) -> Vec<String> {
    let t = line.trim();
    if !t.starts_with('|') || !t.ends_with('|') {
        return Vec::new();
    }
    t[1..t.len() - 1]
        .split('|')
        .map(|c| c.trim().to_string())
        .collect()
}

/// The `Cn` tier of a `tier · stage` cell.
fn cell_tier(cell: &str, _ln: usize) -> Option<u8> {
    let t = cell.trim();
    let c = t.strip_prefix('C')?;
    let digits: String = c.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    digits.parse::<u8>().ok()
}

fn declare(parsed: &mut ParsedSpecDag, id: &str, tier: u8, ln: usize) {
    match parsed.known.get(id) {
        Some(&t) if t != tier => parsed.errors.push(format!(
            "line {}: `{id}` redeclared at C{tier} (declared C{t})",
            ln + 1
        )),
        Some(_) => {}
        None => {
            parsed.known.insert(id.to_string(), tier);
            parsed.dag.add_node(id, tier, NodeKind::Instrument);
        }
    }
}

fn edge(parsed: &mut ParsedSpecDag, from: &str, to: &str, ln: usize) {
    if !parsed.known.contains_key(to) {
        parsed.errors.push(format!(
            "line {}: dependency `{to}` is never declared",
            ln + 1
        ));
        return;
    }
    if to == KS_NODE {
        parsed.ks_edges += 1;
    }
    parsed.dag.add_edge(from, to);
}

// `known` lives on ParsedSpecDag — declared here to keep `SpecDag` untouched.
impl ParsedSpecDag {
    /// The declared node ids → tier (populated by [`parse_spec_tables`]).
    pub fn declared(&self) -> &BTreeMap<String, u8> {
        &self.known
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{ContractRef, ContractRefKind};

    const TABLE: &str = "\
| Scope item | Section | Tier · first stage | C0 slice node(s) (stage) | Core contracts depended on | Extension contracts depended on | ADRs |
|---|---|---|---|---|---|---|
| `R-2.1.1` | §2 | C0 · S1 | whole item | — | — | ADR-1 |
| `R-2.1.2` | §3.1 | C0 · S0→S1 | whole item | item → KS, R-2.1.1 | — | ADR-2 |
| `R-2.2.3` | §5a.3 | C1 · S2 | `R-2.2.3⁰ᵃ` (S1): checkpoint view | `R-2.2.3⁰ᵃ` → KS, R-2.1.1; item → R-2.2.3⁰ᵃ, R-2.1.1 | — | ADR-3 |
| `R-2.9.5` | §5h | C4 · S6 | whole item | item → KS, R-2.1.1 | R-2.6.3¹ (C1) | ADR-4 |
| `R-2.6.3¹` | `R-2.6.3` (C3) | C1 · S4 | content | R-2.1.1, R-2.2.3⁰ᵃ | ADR-5 |
";

    #[test]
    fn parses_items_slices_and_edges() {
        let p = parse_spec_tables(TABLE);
        assert!(p.errors.is_empty(), "{:?}", p.errors);
        // nodes: KS + 4 items + R-2.6.3 (slice-parent cell) + R-2.2.3⁰ᵃ + R-2.6.3¹
        assert_eq!(p.declared().len(), 8);
        assert_eq!(p.declared()["R-2.2.3⁰ᵃ"], 0);
        assert_eq!(p.declared()["R-2.6.3¹"], 1);
        assert_eq!(p.declared()["R-2.9.5"], 4);
        assert!(p.dag.edge_count() > 0);
    }

    #[test]
    fn upward_edge_is_a_tier_violation() {
        // A C0 item depending on a C1 contract is an X2 violation.
        let mut p = parse_spec_tables(TABLE);
        p.dag
            .add_node("contract:dialect:upper/1", 1, NodeKind::Instrument);
        p.dag.add_edge("R-2.1.2", "contract:dialect:upper/1");
        let r = check(&p.dag);
        assert_eq!(
            r.tier_violations,
            vec![Edge {
                from: "R-2.1.2".into(),
                to: "contract:dialect:upper/1".into()
            }]
        );
    }

    #[test]
    fn a_cycle_is_reported() {
        let mut p = parse_spec_tables(TABLE);
        p.dag.add_edge("R-2.1.1", "R-2.1.2"); // R-2.1.2 → R-2.1.1 already exists
        let r = check(&p.dag);
        assert!(!r.cycles.is_empty());
        assert!(r.cycles.contains(&"R-2.1.1".to_string()));
        assert!(r.cycles.contains(&"R-2.1.2".to_string()));
    }

    #[test]
    fn dangling_reference_is_a_parse_error() {
        let bad = "| `R-9.9.9` | §x | C0 · S1 | whole item | item → KS, R-8.8.8 | — | A |\n";
        let p = parse_spec_tables(bad);
        assert_eq!(p.errors.len(), 1);
        assert!(p.errors[0].contains("R-8.8.8"));
    }

    #[test]
    fn class_and_plugin_nodes_extend_the_graph() {
        let classes = vec![ClassDagInput {
            class_id: "control_strategy".to_string(),
            tier: 0,
            depends_on: vec![
                ContractRef::dialect("hir/1", "*"),
                ContractRef::record_kind("variant", "1"),
            ],
        }];
        let plugins = vec![PluginDagInput {
            plugin_id: "hh/test".to_string(),
            tier: 0,
            depends_on: vec![
                ContractRef::class_contract("control_strategy", ">=1.0"),
                ContractRef::protocol_binding("plugin_abi/1", "1"),
            ],
        }];
        let r = spec_dag_check(TABLE, &classes, &plugins);
        assert!(r.parse_errors.is_empty(), "{:?}", r.parse_errors);
        assert!(r.unknown_refs.is_empty(), "{:?}", r.unknown_refs);
        assert!(
            r.tier_violations().is_empty(),
            "{:?}",
            r.inner.tier_violations
        );
        assert!(r.cycles().is_empty());
    }

    #[test]
    fn a_c0_plugin_depending_on_hosting_is_a_violation() {
        // AC-12-adjacent: `hh-hosting/1` defines at C2 — a C0 plugin's
        // depends_on naming it is a tier violation (and fails compat anyway).
        let plugins = vec![PluginDagInput {
            plugin_id: "local/bad".to_string(),
            tier: 0,
            depends_on: vec![ContractRef::new(
                ContractRefKind::ProtocolBinding,
                "hh-hosting/1",
                "*",
            )],
        }];
        let r = spec_dag_check(TABLE, &[], &plugins);
        assert_eq!(r.tier_violations().len(), 1);
        assert_eq!(r.inner.tier_violations[0].from, "plugin:local/bad");
    }
}
