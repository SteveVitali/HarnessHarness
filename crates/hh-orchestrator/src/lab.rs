//! `lab/delegation-v1` first execution (§5e.3 C3 tail; ADR-0186 D6;
//! matched-budget conditional reporting — AC "C3 values remain
//! matched-budget conditional").
//!
//! The recipe itself is the registered `hh_lab::exemplars::delegation_v1`
//! `ExperimentSpec` — this module is the *execution* half: one arm's
//! topology instantiation through the real `spawn` path, the per-arm
//! usage fold, and the matched-budget gate every comparison passes
//! (`MatchSpec` per arm; an unmatchable arm is a typed refusal —
//! `MissingMatchSpec`/`IncommensurableMatch` — never an unverifiable
//! claim).

use std::collections::{BTreeMap, BTreeSet};

use hh_budget::matchspec::MatchSpec;
use hh_budget::quantity::ResourceVector;
use hh_ledger::manifest::EventRef;
use hh_ledger::store::Store;
use hh_monitor::table::HandleTable;
use hh_subagent::spawn::{fold_children, EnvDeriver, SpawnHook};
use hh_subagent::types::*;
use hh_wire::json::Json;

use crate::orchestrator::{Orchestrator, ParentBinding, TopologyRun};
use crate::topology::{TopologyPreset, WaitPolicy};

/// One arm's execution record — what the first run produced (the
/// `budget_match` evidence the conditional report cites).
#[derive(Debug, Clone)]
pub struct ArmExecution {
    /// The recipe arm id (`arm:{topology}:{model}`).
    pub arm_id: String,
    /// The topology level (`t0 | t1 | t2 | t6 | …`).
    pub topology: String,
    /// The children this arm spawned (empty for T0).
    pub children: Vec<String>,
    /// The arm's total usage (parent + all children — `matched_total`
    /// sums the whole arm, ADR-0041 M3: child spend is subject spend).
    pub usage: ResourceVector,
    /// The primary dimensions the arm's budget tree declares (a zero spend
    /// is evidence only for a declared/accounted dimension — absent is
    /// `IncommensurableMatch`, never zero).
    pub accounted_dimensions: BTreeSet<hh_ontology::dimensions::DimensionId>,
    /// `coordination.lost_write_count` primary — folded from the arm's
    /// `MergeReport`s (`0` arms merge nothing).
    pub lost_write_count: u64,
    /// Whether every spawned child reached a terminal row.
    pub all_terminal: bool,
}

/// The conditional-report gate — an arm without a `MatchSpec` refuses
/// `MissingMatchSpec`; an arm whose accounting cannot cover the spec's
/// dimensions refuses `IncommensurableMatch` (§5e.3 "refuse arms without
/// a `MatchSpec`"; ADR-0186 D6's matched-total rule).
#[derive(Debug, Clone, PartialEq)]
pub enum LabRefusal {
    /// The arm carried no `MatchSpec`.
    MissingMatchSpec { arm_id: String },
    /// The arm's recorded usage lacks a matched dimension — the
    /// comparison would read zeros as truth.
    IncommensurableMatch { arm_id: String, detail: String },
}

/// `execute_arm` — one arm through the real spawn path: plan the preset
/// into specs, spawn each admitted stage, and fold the arm's usage once
/// the caller has driven the children to terminal (`collect` completes
/// them). T0 arms spawn nothing (the solo baseline).
///
/// The parameter list is intentionally explicit: each input is a distinct
/// authority/fault seam the lab arm must wire through `spawn` verbatim.
#[allow(clippy::too_many_arguments)]
pub fn execute_arm(
    store: &mut Store,
    handles: &HandleTable,
    binding: &ParentBinding,
    mut env_driver: Option<&mut (dyn EnvDeriver + 'static)>,
    decision: &EventRef,
    arm_id: &str,
    preset: &TopologyPreset,
    stages: Vec<SubagentSpec>,
    wait: WaitPolicy,
    mut hook: Option<&mut (dyn SpawnHook + 'static)>,
) -> Result<ArmExecution, SpawnError> {
    let mut run = Orchestrator::plan(preset, stages, wait)?;
    // T1/T6 drain fully; T2 gates each stage on the previous terminal —
    // `spawn_next` returns `None` until then (the caller drives children
    // to `finished` between calls and re-asks). The env driver is
    // re-borrowed per stage.
    loop {
        match Orchestrator::spawn_next(
            store,
            handles,
            binding,
            env_driver.as_deref_mut(),
            decision,
            &mut run,
            hook.as_deref_mut(),
        ) {
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(e) => return Err(e),
        }
    }
    fold_arm_usage(store, binding, arm_id, preset, &run)
}

/// `execute_arm_spawn` — one `spawn_next` step (the T2 stage gate and the
/// T1/T6 queue; caller loops until `None`, driving children to terminal
/// between calls). The env driver is offered per call.
pub fn execute_arm_spawn<'c>(
    store: &'c mut Store,
    handles: &'c HandleTable,
    binding: &'c ParentBinding,
    env_driver: Option<&'c mut (dyn EnvDeriver + 'static)>,
    decision: &'c EventRef,
    run: &mut TopologyRun,
    hook: Option<&'c mut (dyn SpawnHook + 'static)>,
) -> Result<Option<Spawned>, SpawnError> {
    Orchestrator::spawn_next(store, handles, binding, env_driver, decision, run, hook)
}

/// The arm's usage fold — `matched_total` sums parent + children (child
/// spend is subject spend — ADR-0041 M3). Reads `control.subagent.result`
/// `usage` members plus the parent's own spend events (the caller folds
/// the parent side; this returns the children's contribution keyed by
/// child).
pub fn fold_arm_usage(
    store: &Store,
    binding: &ParentBinding,
    arm_id: &str,
    preset: &TopologyPreset,
    run: &TopologyRun,
) -> Result<ArmExecution, SpawnError> {
    let mut usage = ResourceVector::zero();
    let mut accounted_dimensions = BTreeSet::new();
    let budget_events = store
        .events(&binding.parent_run_id)
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    let budget_tree =
        hh_budget::tree::BudgetTree::project(binding.parent_run_id.clone(), budget_events)
            .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    for node in budget_tree.nodes.values() {
        for key in node.node.spec.dimensions.keys() {
            match key {
                hh_ontology::dimensions::DimensionKey::Primary(d) => {
                    accounted_dimensions.insert(*d);
                }
                hh_ontology::dimensions::DimensionKey::Derived(d) => {
                    for c in d.components() {
                        accounted_dimensions.insert(*c);
                    }
                }
            }
        }
    }
    let mut lost_write_count = 0u64;
    for e in budget_events.iter() {
        if e.class != "control.merge.completed" {
            continue;
        }
        let Some(report_ref) = e.payload.get("merge_report_ref").and_then(Json::as_str) else {
            continue;
        };
        let Ok(parsed) = hh_identity::idp::parse_id(report_ref) else {
            continue;
        };
        let Ok(bytes) = store.get_blob(&hh_identity::idp::ContentAddress {
            idp: "idp/1",
            algorithm: "sha256",
            digest: parsed.digest_hex,
            media_type: "application/json".to_string(),
            size: 0,
        }) else {
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        let Ok(report) = hh_wire::json::parse(&text) else {
            continue;
        };
        lost_write_count += report
            .get("lost_write_count")
            .and_then(Json::as_int)
            .map(|v| v.max(0) as u64)
            .unwrap_or(0);
    }
    let mut all_terminal = true;
    let mut children_ids = Vec::new();
    let children = fold_children(store, &binding.parent_run_id)?;
    for (_, spawned) in &run.spawned {
        children_ids.push(spawned.child_run_id.clone());
        let rec = children.iter().find(|(c, _)| c == &spawned.child_run_id);
        match rec {
            Some((_, r)) if r.terminal_class.as_deref() == Some("control.subagent.result") => {
                if let Some(p) = &r.terminal_payload {
                    // The child's accounted spend — `usage.amounts` fold
                    // (primary dimensions only).
                    if let Some(Json::Obj(amounts)) = p.get("usage").and_then(|u| u.get("amounts"))
                    {
                        for (d, a) in amounts {
                            if let (Some(dim), Some(v)) =
                                (hh_ontology::dimensions::DimensionId::parse(d), a.as_int())
                            {
                                usage.add(dim, v);
                            }
                        }
                    }
                }
            }
            Some((_, r)) if r.terminal_class.is_some() => {}
            _ => all_terminal = false,
        }
    }
    Ok(ArmExecution {
        arm_id: arm_id.to_string(),
        topology: preset.as_str().to_string(),
        children: children_ids,
        usage,
        accounted_dimensions,
        lost_write_count,
        all_terminal,
    })
}

/// `match_report(arms, specs)` — the conditional-report gate: every arm
/// must carry a `MatchSpec`; every matched dimension must appear in the
/// arm's recorded usage (an absent dimension reads as missing, never
/// zero). Returns the comparison evidence payload — `budget_match` +
/// per-arm usage — the `ComparisonReport`'s input (the statistical fold
/// is hh-lab's; the orchestrator reports only what matched accounting
/// permits).
pub fn match_report(arms: &[(&ArmExecution, Option<&MatchSpec>)]) -> Result<Json, LabRefusal> {
    let mut arm_rows = Vec::new();
    for (arm, spec) in arms {
        let spec = spec.ok_or_else(|| LabRefusal::MissingMatchSpec {
            arm_id: arm.arm_id.clone(),
        })?;
        for d in &spec.dimensions {
            if !arm.accounted_dimensions.contains(d) {
                return Err(LabRefusal::IncommensurableMatch {
                    arm_id: arm.arm_id.clone(),
                    detail: format!(
                        "dimension {} is absent from the arm's accounting",
                        d.as_str()
                    ),
                });
            }
        }
        arm_rows.push(Json::obj([
            ("arm_id", Json::str(arm.arm_id.clone())),
            ("topology", Json::str(arm.topology.clone())),
            (
                "children",
                Json::Arr(arm.children.iter().map(|c| Json::str(c.clone())).collect()),
            ),
            (
                "accounted_dimensions",
                Json::Arr(
                    arm.accounted_dimensions
                        .iter()
                        .map(|d| Json::str(d.as_str()))
                        .collect(),
                ),
            ),
            (
                "usage",
                Json::Obj(
                    arm.usage
                        .amounts
                        .iter()
                        .map(|(d, v)| (d.as_str().to_string(), Json::Int(*v)))
                        .collect::<BTreeMap<_, _>>(),
                ),
            ),
            (
                "lost_write_count",
                Json::Int(arm.lost_write_count.min(i64::MAX as u64) as i64),
            ),
            ("all_terminal", Json::Bool(arm.all_terminal)),
            ("match_mode", Json::str(match_mode_str(spec))),
        ]));
    }
    Ok(Json::obj([
        ("kind", Json::str("delegation_v1_arm_report")),
        ("label", Json::str("conditional")),
        ("arms", Json::Arr(arm_rows)),
    ]))
}

fn match_mode_str(spec: &MatchSpec) -> &'static str {
    match spec.mode {
        hh_budget::matchspec::MatchMode::MatchedTotal => "matched_total",
        hh_budget::matchspec::MatchMode::MatchedCap => "matched_cap",
        _ => "other",
    }
}
