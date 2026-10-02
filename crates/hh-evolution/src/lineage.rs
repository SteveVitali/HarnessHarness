//! The evolution lineage DAG + attribution over it (R-2.12.1⁴; S6.3b).
//!
//! `LineageDag` folds the campaign view's candidate records into the
//! parent–child DAG: a candidate's `base_ref` names the applied target
//! it was proposed over, so the parent edge is the candidate whose
//! `target_ref` that base equals (the campaign's
//! `base_definition_ref` is the root — `parent = None`). Rejected
//! candidates stay in the DAG (lineage is a record, never a winner's
//! history — G9/AC-R-2.9.5-11).
//!
//! `attribute_lineage` composes the deposited `hh-attribution/1`
//! reports over the DAG: each node's *edge effect* is the signed sum of
//! its report's `effects[].point` (the diff that produced the node is
//! the edge into it), and `edge_share_ppm` is the node's share of the
//! lineage's total |effect| — the honest "which member moved the head"
//! table, never a counterfactual decomposition claim.

use crate::view::CampaignView;
use hh_wire::json::Json;
use std::collections::BTreeMap;

/// One lineage node — a candidate plus the parent edge it was proposed
/// over.
#[derive(Debug, Clone, PartialEq)]
pub struct LineageNode {
    /// The candidate id.
    pub candidate_id: String,
    /// The parent candidate (`None` = proposed over the campaign base).
    pub parent: Option<String>,
    /// The generation depth (0 = direct child of the campaign base).
    pub generation: u32,
    /// The base the candidate's diff applied over.
    pub base_ref: Option<String>,
    /// The applied target the candidate minted (its children's
    /// `base_ref`).
    pub target_ref: Option<String>,
    /// The folded state.
    pub state: String,
    /// The attribution label the validated/accepted evidence earned.
    pub attribution_label: Option<String>,
}

/// The folded lineage DAG.
#[derive(Debug, Clone, Default)]
pub struct LineageDag {
    /// `candidate_id → node` (BTree order = deterministic iteration).
    pub nodes: BTreeMap<String, LineageNode>,
}

impl LineageDag {
    /// `lineage_dag(view)` — fold the campaign's candidate records into
    /// the parent–child DAG. A candidate's parent is the candidate whose
    /// `target_ref` (or, when the parent never activated, whose
    /// `base_ref`) its `base_ref` equals; a `base_ref` equal to the
    /// campaign spec's base is a root. Cycles are impossible by
    /// construction (`base_ref` resolves against *earlier* registrations
    /// only — the intake order is the fold order).
    pub fn from_view(view: &CampaignView, base_definition_ref: &str) -> LineageDag {
        let mut dag = LineageDag::default();
        // Two passes — the view's `candidates` map is id-sorted, not
        // intake-ordered, so the applied-target index materializes
        // before any parent resolves (a child's id may sort before its
        // parent's).
        let mut applied: BTreeMap<String, String> = BTreeMap::new();
        for (id, rec) in &view.candidates {
            if let Some(t) = &rec.target_ref {
                applied.insert(t.clone(), id.clone());
            }
        }
        let mut parents: BTreeMap<String, Option<String>> = BTreeMap::new();
        for (id, rec) in &view.candidates {
            let parent = rec.base_ref.as_deref().and_then(|b| {
                if b == base_definition_ref {
                    None
                } else {
                    applied.get(b).cloned()
                }
            });
            parents.insert(id.clone(), parent);
        }
        // Generations — a BFS from the roots so a child always lands one
        // generation below its parent even when the parent's node has
        // not been emitted yet.
        let mut generation: BTreeMap<String, u32> = BTreeMap::new();
        let mut queue: std::collections::VecDeque<String> = parents
            .iter()
            .filter(|(_, p)| p.is_none())
            .map(|(id, _)| {
                generation.insert(id.clone(), 0);
                id.clone()
            })
            .collect();
        while let Some(id) = queue.pop_front() {
            let g = generation[&id];
            for (cid, p) in &parents {
                if p.as_deref() == Some(id.as_str()) && !generation.contains_key(cid) {
                    generation.insert(cid.clone(), g + 1);
                    queue.push_back(cid.clone());
                }
            }
        }
        for (id, rec) in &view.candidates {
            dag.nodes.insert(
                id.clone(),
                LineageNode {
                    candidate_id: id.clone(),
                    parent: parents.get(id).cloned().flatten(),
                    generation: generation.get(id).copied().unwrap_or(0),
                    base_ref: rec.base_ref.clone(),
                    target_ref: rec.target_ref.clone(),
                    state: rec.state.clone(),
                    attribution_label: rec.attribution_label.clone(),
                },
            );
        }
        dag
    }

    /// The canonical JSON (`hh-lineage-dag/1` — nodes in content order,
    /// parent edges by ref).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("schema", Json::str("hh-lineage-dag/1")),
            (
                "nodes",
                Json::Arr(
                    self.nodes
                        .values()
                        .map(|n| {
                            let mut m = vec![
                                ("candidate_id", Json::str(&n.candidate_id)),
                                ("generation", Json::Int(n.generation as i64)),
                                ("state", Json::str(&n.state)),
                            ];
                            if let Some(p) = &n.parent {
                                m.push(("parent", Json::str(p)));
                            }
                            if let Some(b) = &n.base_ref {
                                m.push(("base_ref", Json::str(b)));
                            }
                            if let Some(t) = &n.target_ref {
                                m.push(("target_ref", Json::str(t)));
                            }
                            if let Some(l) = &n.attribution_label {
                                m.push(("attribution_label", Json::str(l)));
                            }
                            Json::obj(m)
                        })
                        .collect(),
                ),
            ),
        ])
    }
}

/// `attribute_lineage(dag, reports)` — compose the deposited
/// `hh-attribution/1` reports over the DAG (`candidate_id → report`).
/// A node's edge effect is the signed sum of its report's
/// `effects[].point`; `edge_share_ppm` is the node's share of the
/// lineage-wide |effect| mass. Nodes without a report carry
/// `edge_effect = 0` and `reported = false` — the table is complete
/// over the DAG, never silently truncated.
pub fn attribute_lineage(dag: &LineageDag, reports: &BTreeMap<String, Json>) -> Json {
    let mut effects: BTreeMap<String, i64> = BTreeMap::new();
    for (id, rep) in reports {
        if rep.get("schema").and_then(Json::as_str) != Some("hh-attribution/1") {
            continue;
        }
        let sum: i64 = rep
            .get("effects")
            .and_then(|e| match e {
                Json::Arr(a) => Some(
                    a.iter()
                        .filter_map(|row| row.get("point").and_then(Json::as_int))
                        .sum(),
                ),
                _ => None,
            })
            .unwrap_or(0);
        effects.insert(id.clone(), sum);
    }
    let total_abs: i64 = effects.values().map(|e| e.abs()).sum();
    Json::obj([
        ("schema", Json::str("hh-lineage-attribution/1")),
        (
            "nodes",
            Json::Arr(
                dag.nodes
                    .values()
                    .map(|n| {
                        let eff = effects.get(&n.candidate_id).copied().unwrap_or(0);
                        let mut m = vec![
                            ("candidate_id", Json::str(&n.candidate_id)),
                            ("generation", Json::Int(n.generation as i64)),
                            ("state", Json::str(&n.state)),
                            ("edge_effect", Json::Int(eff)),
                            (
                                "edge_share_ppm",
                                Json::Int(if total_abs == 0 {
                                    0
                                } else {
                                    eff.abs() * 1_000_000 / total_abs
                                }),
                            ),
                            (
                                "reported",
                                Json::Bool(reports.contains_key(&n.candidate_id)),
                            ),
                        ];
                        if let Some(p) = &n.parent {
                            m.push(("parent", Json::str(p)));
                        }
                        if let Some(l) = &n.attribution_label {
                            m.push(("attribution_label", Json::str(l)));
                        }
                        Json::obj(m)
                    })
                    .collect(),
            ),
        ),
    ])
}
