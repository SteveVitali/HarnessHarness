//! The spec-DAG check (spec §2.9.8 "the §4.4 spec-DAG check for the HIR → Hosting ABI edge";
//! AC-A2-6; CC6). The static half landed at S1.1.
//!
//! Three properties are checked over the §2 dependency graph:
//! 1. **tier monotonicity** — a C(n) item depends only on contracts of defining tier ≤ n; an
//!    edge to a strictly higher tier is a `tier_violation` (CC6; X2);
//! 2. **acyclicity** — the item graph is acyclic; any cycle is reported (CC6; X4);
//! 3. **no HIR → Hosting-ABI edge** — no dependency edge runs from any HIR entity to the
//!    Hosting ABI, while `proj_ABI` is **total** on native ledgers (native → the minimum
//!    observable event set) with no reverse edge (§2.7.5; T-LCD-06; ADR-0013 D5).
//!
//! At Stage 1 the graph is the ontology's own §2 constructs; later tickets extend it. The check
//! is executable now: `spec_dag_check()` must report `tier_violations=[]`, `cycles=[]`,
//! `hosting_edges=[]`.

use std::collections::BTreeMap;

/// The kind of a node in the spec DAG — enough to police the HIR → Hosting-ABI edge and
/// `proj_ABI` totality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// An HIR entity / object-level construct.
    Hir,
    /// The Hosting ABI (instrument-level boundary).
    HostingAbi,
    /// A native run ledger (the source of `proj_ABI`).
    NativeLedger,
    /// The minimum observable event set (the target of `proj_ABI`).
    HostedEventSet,
    /// Another instrument-level construct.
    Instrument,
}

/// A node: an id, its defining tier, and its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The stable node id.
    pub id: String,
    /// The defining tier (C0..C4). The ontology is C0.
    pub tier: u8,
    /// The node kind.
    pub kind: NodeKind,
}

/// A directed `depends_on` / projection edge `from → to`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Edge {
    /// The dependent (or projection source).
    pub from: String,
    /// The dependency (or projection target).
    pub to: String,
}

/// The dependency graph.
#[derive(Debug, Clone, Default)]
pub struct SpecDag {
    nodes: BTreeMap<String, Node>,
    edges: Vec<Edge>,
}

impl SpecDag {
    /// Empty graph.
    pub fn new() -> SpecDag {
        SpecDag::default()
    }

    /// Add a node.
    pub fn add_node(&mut self, id: &str, tier: u8, kind: NodeKind) {
        self.nodes.insert(
            id.to_string(),
            Node {
                id: id.to_string(),
                tier,
                kind,
            },
        );
    }

    /// Add an edge `from → to`.
    pub fn add_edge(&mut self, from: &str, to: &str) {
        self.edges.push(Edge {
            from: from.to_string(),
            to: to.to_string(),
        });
    }

    fn kind_of(&self, id: &str) -> Option<NodeKind> {
        self.nodes.get(id).map(|n| n.kind)
    }

    /// Tier violations: an edge whose target has a strictly higher tier than its source (CC6).
    pub fn tier_violations(&self) -> Vec<Edge> {
        let mut out = Vec::new();
        for e in &self.edges {
            if let (Some(a), Some(b)) = (self.nodes.get(&e.from), self.nodes.get(&e.to)) {
                if b.tier > a.tier {
                    out.push(e.clone());
                }
            }
        }
        out.sort();
        out
    }

    /// Cycles: returns each node that participates in a cycle (empty ⇒ acyclic).
    pub fn cycles(&self) -> Vec<String> {
        // Kahn's algorithm; whatever cannot be removed sits on a cycle.
        let mut indeg: BTreeMap<&str, usize> = self.nodes.keys().map(|k| (k.as_str(), 0)).collect();
        let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for e in &self.edges {
            if self.nodes.contains_key(&e.from) && self.nodes.contains_key(&e.to) {
                adj.entry(e.from.as_str()).or_default().push(e.to.as_str());
                *indeg.get_mut(e.to.as_str()).unwrap() += 1;
            }
        }
        let mut queue: Vec<&str> = indeg
            .iter()
            .filter(|(_, d)| **d == 0)
            .map(|(k, _)| *k)
            .collect();
        let mut removed = 0usize;
        while let Some(n) = queue.pop() {
            removed += 1;
            if let Some(succ) = adj.get(n) {
                for &m in succ {
                    let d = indeg.get_mut(m).unwrap();
                    *d -= 1;
                    if *d == 0 {
                        queue.push(m);
                    }
                }
            }
        }
        if removed == self.nodes.len() {
            Vec::new()
        } else {
            indeg
                .iter()
                .filter(|(_, d)| **d > 0)
                .map(|(k, _)| k.to_string())
                .collect()
        }
    }

    /// Edges from an HIR node to the Hosting ABI — must be empty (§2.7.5; T-LCD-06).
    pub fn hosting_edges(&self) -> Vec<Edge> {
        let mut out: Vec<Edge> = self
            .edges
            .iter()
            .filter(|e| {
                self.kind_of(&e.from) == Some(NodeKind::Hir)
                    && self.kind_of(&e.to) == Some(NodeKind::HostingAbi)
            })
            .cloned()
            .collect();
        out.sort();
        out
    }

    /// `proj_ABI` totality: there is a projection edge native-ledger → hosted-event-set and no
    /// reverse edge (no dependency from a hosted construct back into a native ledger) — §2.7.5.
    pub fn proj_abi_total(&self) -> bool {
        let forward = self.edges.iter().any(|e| {
            self.kind_of(&e.from) == Some(NodeKind::NativeLedger)
                && self.kind_of(&e.to) == Some(NodeKind::HostedEventSet)
        });
        let reverse = self.edges.iter().any(|e| {
            self.kind_of(&e.from) == Some(NodeKind::HostedEventSet)
                && self.kind_of(&e.to) == Some(NodeKind::NativeLedger)
        });
        forward && !reverse
    }
}

/// The report `spec_dag_check` produces (AC-A2-6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DagReport {
    /// Edges violating tier monotonicity (must be empty).
    pub tier_violations: Vec<Edge>,
    /// Nodes on a cycle (must be empty).
    pub cycles: Vec<String>,
    /// HIR → Hosting-ABI edges (must be empty).
    pub hosting_edges: Vec<Edge>,
    /// Whether `proj_ABI` is total on native ledgers (must be true).
    pub proj_abi_total: bool,
}

impl DagReport {
    /// Whether every AC-A2-6 static property holds.
    pub fn is_clean(&self) -> bool {
        self.tier_violations.is_empty()
            && self.cycles.is_empty()
            && self.hosting_edges.is_empty()
            && self.proj_abi_total
    }
}

/// Build the S1.1 ontology spec DAG — the §2 constructs with their tiers and dependency edges.
/// Object-level (HIR) constructs are C0; the Hosting ABI and the projection targets are
/// instrument-level. The edges encode the §2 dependency facts (§2.9.5) that bear on AC-A2-6.
pub fn ontology_spec_dag() -> SpecDag {
    let mut g = SpecDag::new();

    // Object-level (HIR) C0 constructs.
    g.add_node("hir.agent_process", 0, NodeKind::Hir);
    g.add_node("hir.control_boundary", 0, NodeKind::Hir);
    g.add_node("hir.harness_artifact", 0, NodeKind::Hir);
    g.add_node("hir.native_ledger", 0, NodeKind::NativeLedger);

    // Instrument-level constructs (higher tier where hosting is concerned).
    g.add_node("instrument.participant_descriptor", 0, NodeKind::Instrument);
    g.add_node("instrument.hosted_event_set", 0, NodeKind::HostedEventSet);
    g.add_node("hosting_abi", 3, NodeKind::HostingAbi);

    // Object-level dependency edges (all within tier 0 — no violation, no cycle).
    // β is a record on AgentProcess.native.
    g.add_edge("hir.agent_process", "hir.control_boundary");
    // A harness artifact is a projection over HIR kinds carried in the ledger.
    g.add_edge("hir.native_ledger", "hir.harness_artifact");

    // proj_ABI: native ledger → the minimum observable event set (total, no reverse edge).
    g.add_edge("hir.native_ledger", "instrument.hosted_event_set");
    // The Hosting ABI's descriptor lowers to ParticipantDescriptor (instrument → instrument).
    g.add_edge("hosting_abi", "instrument.participant_descriptor");

    // Deliberately NO edge hir.* → hosting_abi (T-LCD-06). The check proves its absence.
    g
}

/// Run the spec-DAG check over the S1.1 ontology graph (AC-A2-6).
pub fn spec_dag_check() -> DagReport {
    check(&ontology_spec_dag())
}

/// Run the spec-DAG check over an arbitrary graph.
pub fn check(g: &SpecDag) -> DagReport {
    DagReport {
        tier_violations: g.tier_violations(),
        cycles: g.cycles(),
        hosting_edges: g.hosting_edges(),
        proj_abi_total: g.proj_abi_total(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ontology_dag_is_clean() {
        // AC-A2-6 static half: tier_violations=[], cycles=[], no HIR→Hosting edge, proj_ABI total.
        let report = spec_dag_check();
        assert_eq!(report.tier_violations, Vec::<Edge>::new());
        assert_eq!(report.cycles, Vec::<String>::new());
        assert_eq!(report.hosting_edges, Vec::<Edge>::new());
        assert!(report.proj_abi_total);
        assert!(report.is_clean());
    }

    #[test]
    fn a_hir_to_hosting_edge_is_caught() {
        // The check must FAIL (be non-empty) if someone adds the forbidden edge — the guard
        // that makes the "no HIR → Hosting ABI edge" property load-bearing.
        let mut g = ontology_spec_dag();
        g.add_edge("hir.agent_process", "hosting_abi");
        let report = check(&g);
        assert_eq!(report.hosting_edges.len(), 1);
        assert!(!report.is_clean());
    }

    #[test]
    fn a_tier_violation_is_caught() {
        let mut g = ontology_spec_dag();
        // A C0 HIR node depending on the C3 Hosting ABI is a tier violation (and a hosting edge).
        g.add_edge("hir.control_boundary", "hosting_abi");
        let report = check(&g);
        assert!(!report.tier_violations.is_empty());
    }

    #[test]
    fn a_cycle_is_caught() {
        let mut g = SpecDag::new();
        g.add_node("a", 0, NodeKind::Hir);
        g.add_node("b", 0, NodeKind::Hir);
        g.add_edge("a", "b");
        g.add_edge("b", "a");
        assert_eq!(check(&g).cycles.len(), 2);
    }

    #[test]
    fn a_reverse_proj_abi_edge_breaks_totality() {
        let mut g = ontology_spec_dag();
        g.add_edge("instrument.hosted_event_set", "hir.native_ledger");
        assert!(!check(&g).proj_abi_total);
    }
}
