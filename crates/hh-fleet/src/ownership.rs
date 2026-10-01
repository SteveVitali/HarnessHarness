//! The per-fleet ownership graph (§5i.1 #4) — vertices are `spec.agents`,
//! edges are `spec.ownership` plus every durable `control.work_item.
//! owner_changed{agent, owner}` row. `resolved_owner(item)` walks the item's
//! owner through the graph to the effective owner; `set_owner`/`handoff`
//! validate `cross_fleet`/`cycle`/`owner_unknown` before any row lands
//! (`GraphError`s are typed refusals, never warnings).

use std::collections::BTreeMap;

use crate::errors::FleetError;
use crate::spec::FleetSpec;

/// `OwnershipGraph` — the folded edge set (`child → owner`).
#[derive(Debug, Clone, Default)]
pub struct OwnershipGraph {
    /// `agent_id → owner_agent_id`.
    pub edges: BTreeMap<String, String>,
}

impl OwnershipGraph {
    /// The declared graph from a spec (`spec.ownership` verbatim — every
    /// vertex/endpoint must be a declared agent, and the graph must be
    /// acyclic at open).
    pub fn from_spec(spec: &FleetSpec) -> Result<OwnershipGraph, FleetError> {
        let g = OwnershipGraph {
            edges: spec.ownership.clone(),
        };
        for (a, o) in &g.edges {
            if !spec.agents.contains(a) {
                return Err(FleetError::OwnerUnknown {
                    item: a.clone(),
                    agent: a.clone(),
                });
            }
            if !spec.agents.contains(o) {
                return Err(FleetError::OwnerUnknown {
                    item: a.clone(),
                    agent: o.clone(),
                });
            }
        }
        // Cycle check across the whole declared graph.
        for a in spec.agents.clone() {
            g.check_cycle(&a)?;
        }
        Ok(g)
    }

    /// Fold an `owner_changed{agent, owner}` row — the durable edge update.
    pub fn apply(&mut self, agent: &str, owner: &str) {
        self.edges.insert(agent.to_string(), owner.to_string());
    }

    /// `resolved_owner(agent)` — walk `agent → owner → owner → …` to the
    /// top (the top-level owner resolves to themselves — the "latest
    /// ownership chain's top-level owner" of §5i.1 #10's accountability
    /// membership and the `resolve_owner` of #4's handoff).
    pub fn resolve_owner(&self, agent: &str) -> String {
        let mut cur = agent.to_string();
        let mut seen = std::collections::BTreeSet::new();
        while let Some(next) = self.edges.get(&cur) {
            if next == &cur || !seen.insert(next.clone()) {
                break; // defensive — cycles are refused at set/handoff
            }
            cur = next.clone();
        }
        cur
    }

    /// `has_path(from, to)` — is `to` reachable walking owners from `from`?
    /// (the `set_owner` cycle check: `has_path(new_owner, agent)`).
    pub fn has_path(&self, from: &str, to: &str) -> bool {
        let mut cur = from.to_string();
        let mut seen = std::collections::BTreeSet::new();
        loop {
            if cur == to {
                return true;
            }
            match self.edges.get(&cur) {
                Some(next) if seen.insert(next.clone()) => cur = next.clone(),
                _ => return false,
            }
        }
    }

    /// `check_cycle(vertex)` — the open/`set_owner` validation. Follow at
    /// least ONE edge before testing re-entry (`has_path(v, v)` is
    /// trivially true at step 0 — a vertex always "reaches" itself with
    /// zero hops).
    pub fn check_cycle(&self, vertex: &str) -> Result<(), FleetError> {
        let cyclic = self
            .edges
            .get(vertex)
            .map(|first| self.has_path(first, vertex))
            .unwrap_or(false);
        if cyclic {
            // Reconstruct the cycle for the error's `path` member.
            let mut path = vec![vertex.to_string()];
            let mut cur = vertex.to_string();
            while let Some(next) = self.edges.get(&cur) {
                path.push(next.clone());
                if next == vertex {
                    break;
                }
                cur = next.clone();
                if path.len() > self.edges.len() + 1 {
                    break;
                }
            }
            return Err(FleetError::Cycle {
                run: String::new(),
                item: vertex.to_string(),
                path: path.join(">"),
            });
        }
        Ok(())
    }

    /// `set_owner` validation — `cross_fleet` (an owner outside `agents`),
    /// `owner_unknown` (the subject outside `agents`), `cycle`
    /// (`has_path(new_owner, agent)`), no-op detection (`same_owner`).
    /// Returns `Ok(())` when the edge is admissible.
    pub fn validate_set_owner(
        &self,
        spec: &FleetSpec,
        agent: &str,
        new_owner: &str,
    ) -> Result<(), FleetError> {
        if !spec.agents.iter().any(|a| a == agent) {
            return Err(FleetError::OwnerUnknown {
                item: agent.to_string(),
                agent: agent.to_string(),
            });
        }
        if !spec.agents.iter().any(|a| a == new_owner) {
            return Err(FleetError::CrossFleet {
                run: String::new(),
                item: agent.to_string(),
                owner: new_owner.to_string(),
            });
        }
        // The edge `agent → new_owner` closes a cycle iff `new_owner`
        // already reaches `agent` (walking owners — follow ≥1 edge;
        // `has_path(x, x)` is trivially true at step 0).
        let mut with = self.clone();
        with.edges.insert(agent.to_string(), new_owner.to_string());
        if with.has_path(new_owner, agent) {
            let mut path = vec![agent.to_string()];
            let mut cur = agent.to_string();
            while let Some(next) = with.edges.get(&cur) {
                path.push(next.clone());
                if next == agent {
                    break;
                }
                cur = next.clone();
            }
            return Err(FleetError::Cycle {
                run: String::new(),
                item: agent.to_string(),
                path: path.join(">"),
            });
        }
        Ok(())
    }
}

/// `expand_owners(agent)` — the ownership chain `[agent, owner(agent), …]`
/// used by `resolve_owner`'s reporting arm.
pub fn owner_chain(graph: &OwnershipGraph, agent: &str) -> Vec<String> {
    let mut out = vec![agent.to_string()];
    let mut cur = agent.to_string();
    let mut seen = std::collections::BTreeSet::new();
    while let Some(next) = graph.edges.get(&cur) {
        if !seen.insert(next.clone()) {
            break;
        }
        out.push(next.clone());
        cur = next.clone();
    }
    out
}
