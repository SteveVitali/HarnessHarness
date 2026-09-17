//! `trace_map` / `trace(bundle, locator)` (§3.2.4/§3.2.7): every emitted plan element —
//! every control node, every tool binding, every policy row, every validator binding —
//! carries `{hir_node_ids[], rule_ids[], target_rule_ids[]}`. The map is **total** over
//! the emitted set (AC-CP-03): an unresolvable locator is a typed `TraceError`, never an
//! empty answer.

use std::collections::BTreeMap;

use crate::errors::TraceError;
use crate::plan::{PlanNode, PlanNodePayload, RuntimePlan};

/// One trace entry — the provenance triple an emitted element carries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TraceEntry {
    /// The source HIR nodes (semantic ids).
    pub hir_node_ids: Vec<String>,
    /// The conditioned rule ids that produced/shape this element.
    pub rule_ids: Vec<String>,
    /// The target-side rule ids (stage-4 produced; `[]` at Stage 1).
    pub target_rule_ids: Vec<String>,
}

/// `trace_map: map<locator, {hir_node_ids[], rule_ids[], target_rule_ids[]}>` (§3.2.4).
/// The locator grammar is `control/<i>(/body|/then|/else/<j>)*`, `tools/<i>(/surface)?`,
/// `policies/<table>/<i>`, `validators/<i>`, `budget`, `context_policy`, `slots/<name>`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TraceMap {
    /// locator → entry.
    pub entries: BTreeMap<String, TraceEntry>,
}

/// Build the total trace map for a lowered plan — every emitted element gets an entry.
pub fn build_trace_map(plan: &RuntimePlan) -> TraceMap {
    let mut entries: BTreeMap<String, TraceEntry> = BTreeMap::new();
    for (i, node) in plan.control.iter().enumerate() {
        trace_node(node, &format!("control/{i}"), &mut entries);
    }
    for (i, t) in plan.tools.iter().enumerate() {
        let mut e = TraceEntry {
            hir_node_ids: vec![t.capability.semantic_id.clone()],
            ..Default::default()
        };
        entries.insert(format!("tools/{i}"), e.clone());
        if t.surface.is_some() {
            e.rule_ids = Vec::new();
            entries.insert(format!("tools/{i}/surface"), e);
        }
    }
    for (i, p) in plan.policies.permissions.iter().enumerate() {
        entries.insert(
            format!("policies/permissions/{i}"),
            TraceEntry {
                hir_node_ids: vec![p.permission.semantic_id.clone()],
                ..Default::default()
            },
        );
    }
    for (i, r) in plan.policies.effect_classes.iter().enumerate() {
        entries.insert(
            format!("policies/effect_classes/{i}"),
            TraceEntry {
                hir_node_ids: vec![r.capability.clone()],
                ..Default::default()
            },
        );
    }
    for (i, b) in plan.policies.budgets.iter().enumerate() {
        entries.insert(
            format!("policies/budgets/{i}"),
            TraceEntry {
                hir_node_ids: vec![b.budget.semantic_id.clone()],
                ..Default::default()
            },
        );
    }
    for (i, v) in plan.validators.iter().enumerate() {
        entries.insert(
            format!("validators/{i}"),
            TraceEntry {
                hir_node_ids: vec![v.validator.semantic_id.clone()],
                rule_ids: v.required_by.clone(),
                target_rule_ids: Vec::new(),
            },
        );
    }
    if let Some(b) = &plan.budget {
        entries.insert(
            "budget".to_string(),
            TraceEntry {
                hir_node_ids: vec![b.budget.semantic_id.clone()],
                ..Default::default()
            },
        );
    }
    for (name, s) in &plan.bound_slots {
        // A bound slot traces to the `AgentProcess` node that declares the binding; the
        // pinned variant version lives on the plan's `bound_slots` row itself.
        entries.insert(
            format!("slots/{name}"),
            TraceEntry {
                hir_node_ids: vec![s.declared_on.clone()],
                rule_ids: Vec::new(),
                target_rule_ids: Vec::new(),
            },
        );
    }
    if plan.context_policy.is_some() {
        // `context_policy` shares the bound slot's entry — the locator is an alias.
        if let Some(e) = entries.get("slots/context_policy").cloned() {
            entries.insert("context_policy".to_string(), e);
        }
    }
    TraceMap { entries }
}

fn trace_node(node: &PlanNode, locator: &str, out: &mut BTreeMap<String, TraceEntry>) {
    out.insert(
        locator.to_string(),
        TraceEntry {
            hir_node_ids: vec![node.hir_node_id.clone()],
            ..Default::default()
        },
    );
    match &node.payload {
        PlanNodePayload::Loop(l) => {
            for (j, c) in l.body.iter().enumerate() {
                trace_node(c, &format!("{locator}/body/{j}"), out);
            }
        }
        PlanNodePayload::BranchOnValidator(b) => {
            for (j, c) in b.then_body.iter().enumerate() {
                trace_node(c, &format!("{locator}/then/{j}"), out);
            }
            for (j, c) in b.else_body.iter().enumerate() {
                trace_node(c, &format!("{locator}/else/{j}"), out);
            }
        }
        _ => {}
    }
}

/// `trace(bundle, locator)` (§3.2.7) — `TraceError` on an unresolvable locator.
pub fn trace<'m>(map: &'m TraceMap, locator: &str) -> Result<&'m TraceEntry, TraceError> {
    map.entries.get(locator).ok_or_else(|| TraceError {
        locator: locator.to_string(),
    })
}
