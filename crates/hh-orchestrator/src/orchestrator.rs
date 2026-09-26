//! The `orchestrator` component class (§5e.3 C3; ADR-0186). Binds a
//! `TopologyPreset` into per-child `SubagentSpec`s (`topology_ref`,
//! `delegation_reason`, `wait.mode`, `on_parent_end`, `messaging_policy`)
//! and executes them through `hh-subagent`'s `spawn` — the one spawn path
//! for every subagent surface (CF-394/407/451: no second spawn schema
//! exists). Wait reduction `{all, any, quorum(k)}`, T6
//! `detach_to_child`, and parent-mediated messaging live here — the C1
//! kernel knows only `wait.mode` and rows.

use std::collections::{BTreeMap, VecDeque};

use hh_ledger::manifest::EventRef;
use hh_ledger::store::{Lease, Store};
use hh_monitor::delegate::LiveCoords;
use hh_monitor::handle::HandleId;
use hh_monitor::table::HandleTable;
use hh_ontology::control::Owner;
use hh_wire::json::Json;

use hh_subagent::spawn::{fold_children, EnvDeriver, SpawnCtx, SpawnHook};
use hh_subagent::types::*;
use hh_subagent::{messaging, ownership::OwnershipTable, result, spawn as kernel_spawn};

use crate::topology::{TopologyPreset, WaitPolicy};

/// The parent seam the orchestrator binds once per run — owned clones; a
/// `SpawnCtx` is minted per spawn call (the kernel op stays a call, never
/// a held capability).
pub struct ParentBinding {
    /// The parent run.
    pub parent_run_id: String,
    /// The parent's writer lease.
    pub parent_lease: Lease,
    /// The parent handle children delegate from.
    pub parent_handle_id: HandleId,
    /// The parent's `BudgetNode`.
    pub parent_budget_id: String,
    /// The parent's live environment handle (derive requests).
    pub parent_env_id: Option<String>,
    /// The parent's valid ownerships.
    pub ownerships: OwnershipTable,
    /// The parent's sealed tool table (`supplies.tools ⊆`).
    pub tool_table: Vec<hh_hir::refs::Ref>,
    /// The parent's `delegation_depth` gauge level.
    pub parent_depth: u64,
    /// The lease-holder spelling.
    pub holder: String,
    /// `delegate`'s liveness coordinates.
    pub coords: LiveCoords,
    /// `reserve(spawns,1)` TTL.
    pub reserve_ttl_ms: u64,
    /// The spawn decision owner (`code` for orchestrator-driven plans —
    /// the reason is a declared label, not a `model_claim`).
    pub owner: Owner,
}

impl ParentBinding {
    /// The kernel context for one spawn.
    fn spawn_ctx<'a>(
        &'a self,
        store: &'a mut Store,
        handles: &'a HandleTable,
        env_driver: Option<&'a mut (dyn EnvDeriver + 'static)>,
        decision: &'a EventRef,
        hook: Option<&'a mut (dyn SpawnHook + 'static)>,
    ) -> SpawnCtx<'a> {
        SpawnCtx {
            store,
            handles,
            parent_run_id: &self.parent_run_id,
            parent_lease: &self.parent_lease,
            parent_handle_id: &self.parent_handle_id,
            parent_budget_id: &self.parent_budget_id,
            parent_env_id: self.parent_env_id.as_deref(),
            env_driver,
            parent_ownerships: &self.ownerships,
            parent_tool_table: &self.tool_table,
            parent_depth: self.parent_depth,
            decision: decision.clone(),
            owner: self.owner,
            holder: &self.holder,
            coords: self.coords.clone(),
            reserve_ttl_ms: self.reserve_ttl_ms,
            hook,
        }
    }
}

/// A bound topology run — the specs as spawned plus the runtime fold.
pub struct TopologyRun {
    /// The preset.
    pub preset: TopologyPreset,
    /// The per-stage bound specs (spawn order).
    pub specs: Vec<SubagentSpec>,
    /// The spawn order still owed (pipeline stages gate on the previous
    /// stage's terminal; T1's queue drains unconditionally).
    pub pending: VecDeque<usize>,
    /// Spawned children `(stage_index, Spawned)`.
    pub spawned: Vec<(usize, Spawned)>,
    /// The wait reduction.
    pub wait: WaitPolicy,
    /// Stage results `(stage_index, control.subagent.result payload)`.
    pub stage_results: Vec<(usize, Json)>,
}

/// The `orchestrator` class — stateless over the durable fold (its state
/// is the ledger's; a restore re-folds `TopologyRun` from `spawned` rows —
/// the `topology_ref` member names the preset, `stage_index` the stage).
pub struct Orchestrator;

impl Orchestrator {
    /// Bind a preset into per-child specs — sets `topology_ref`,
    /// `delegation_reason` (the preset's declared reason — the caller may
    /// override to another closed-set member; `None` is refused),
    /// `wait.mode`, `on_parent_end`, `messaging_policy`, `stage_index`.
    /// `base` is the caller's spec template; `stages` are the per-child
    /// specs built from it (goal/grants/supplies are the caller's — the
    /// orchestrator never invents them).
    pub fn plan(
        preset: &TopologyPreset,
        mut stages: Vec<SubagentSpec>,
        wait: WaitPolicy,
    ) -> Result<TopologyRun, SpawnError> {
        if !preset.implemented() {
            return Err(SpawnError::Refused(SpawnRefused::ModeUnsupported {
                detail: format!(
                    "topology {} is declared, not this slice's arm",
                    preset.as_str()
                ),
            }));
        }
        let expected = match preset {
            TopologyPreset::T0Single => Some(0),
            TopologyPreset::T1OrchestratorWorker { fan_out } => Some(*fan_out as usize),
            TopologyPreset::T2Pipeline { stages } => Some(*stages as usize),
            TopologyPreset::T6BackgroundDetached => Some(1),
            _ => None,
        };
        if let Some(n) = expected {
            if stages.len() != n {
                return Err(SpawnError::Refused(SpawnRefused::ModeUnsupported {
                    detail: format!(
                        "topology {} binds {} stage(s), got {}",
                        preset.topology_ref(),
                        n,
                        stages.len()
                    ),
                }));
            }
        }
        let stage_count = stages.len();
        let (unreachable_wait, wait_label) = match wait {
            WaitPolicy::All => (false, "all".to_string()),
            WaitPolicy::Any => (stage_count == 0, "any".to_string()),
            WaitPolicy::Quorum(k) => (k == 0 || k as usize > stage_count, format!("quorum({k})")),
        };
        if unreachable_wait {
            return Err(SpawnError::Refused(SpawnRefused::ModeUnsupported {
                detail: format!(
                    "wait {wait_label} cannot be satisfied by {stage_count} child stage(s)"
                ),
            }));
        }
        let declared = preset.delegation_reason();
        for (i, s) in stages.iter_mut().enumerate() {
            s.topology_ref = Some(preset.topology_ref());
            s.delegation_reason = declared;
            s.wait_mode = preset.wait_mode();
            s.on_parent_end = preset.on_parent_end();
            s.stage_index = Some(i as u64);
            s.messaging_policy = Some(preset.messaging_policy());
        }
        let pending: VecDeque<usize> = (0..stages.len()).collect();
        Ok(TopologyRun {
            preset: *preset,
            specs: stages,
            pending,
            spawned: Vec::new(),
            wait,
            stage_results: Vec::new(),
        })
    }

    /// `spawn_next` — the next stage the topology admits. T1/T6 drain
    /// `pending` unconditionally; T2 admits stage `i` only once stage
    /// `i-1` has a terminal row (the pipeline's data-flow edge — the next
    /// stage's `supplies` were bound at `plan` time).
    pub fn spawn_next<'c>(
        store: &'c mut Store,
        handles: &'c HandleTable,
        binding: &'c ParentBinding,
        env_driver: Option<&'c mut (dyn EnvDeriver + 'static)>,
        decision: &'c EventRef,
        run: &mut TopologyRun,
        hook: Option<&'c mut (dyn SpawnHook + 'static)>,
    ) -> Result<Option<Spawned>, SpawnError> {
        let stage = match run.pending.front() {
            None => return Ok(None),
            Some(&s) => s,
        };
        if matches!(run.preset, TopologyPreset::T2Pipeline { .. }) && stage > 0 {
            // The previous stage must have a terminal row first.
            let prev_child = run
                .spawned
                .iter()
                .find(|(i, _)| *i == stage - 1)
                .map(|(_, s)| s.child_run_id.clone());
            match prev_child {
                None => return Ok(None),
                Some(c) => {
                    let terminal = fold_children(store, &binding.parent_run_id)?
                        .into_iter()
                        .any(|(cid, r)| cid == c && r.terminal_class.is_some());
                    if !terminal {
                        return Ok(None);
                    }
                }
            }
        }
        run.pending.pop_front();
        let spec = run.specs[stage].clone();
        let mut ctx = binding.spawn_ctx(store, handles, env_driver, decision, hook);
        let spawned = kernel_spawn::spawn(&mut ctx, &spec)?;
        run.spawned.push((stage, spawned.clone_shallow()));
        Ok(Some(spawned))
    }

    /// `collect` — drain finished children into `stage_results`
    /// (folds `control.subagent.result` payloads keyed by stage).
    pub fn collect(
        store: &mut Store,
        binding: &ParentBinding,
        run: &mut TopologyRun,
    ) -> Result<Vec<String>, SpawnError> {
        let specs: BTreeMap<String, SubagentSpec> = run
            .spawned
            .iter()
            .map(|(i, s)| (s.child_run_id.clone(), run.specs[*i].clone()))
            .collect();
        let completed = result::drain_child_terminal(
            store,
            &binding.parent_run_id,
            &binding.parent_lease,
            &specs,
        )?;
        // Fold result payloads per stage.
        let children = fold_children(store, &binding.parent_run_id)?;
        for (stage, spawned) in &run.spawned {
            if run.stage_results.iter().any(|(i, _)| i == stage) {
                continue;
            }
            if let Some((_, rec)) = children.iter().find(|(c, _)| c == &spawned.child_run_id) {
                if let Some(p) = &rec.terminal_payload {
                    if rec.terminal_class.as_deref() == Some("control.subagent.result") {
                        run.stage_results.push((*stage, p.clone()));
                    }
                }
            }
        }
        Ok(completed)
    }

    /// Whether the run's wait policy is satisfied (terminals seen vs the
    /// reduction).
    pub fn wait_satisfied(
        store: &Store,
        binding: &ParentBinding,
        run: &TopologyRun,
    ) -> Result<bool, SpawnError> {
        let terminals = fold_children(store, &binding.parent_run_id)?
            .into_iter()
            .filter(|(c, r)| {
                r.terminal_class.is_some() && run.spawned.iter().any(|(_, s)| &s.child_run_id == c)
            })
            .count();
        Ok(run.wait.satisfied(terminals, run.spawned.len()))
    }

    /// T6 — `detach_to_child`: the child leaves the parent's coordination
    /// (`control.subagent.detached`); its terminal still reaches the
    /// retained `child_terminal` subscription (C-3 proof).
    pub fn detach_to_child(
        store: &mut Store,
        binding: &ParentBinding,
        child_run_id: &str,
        decision: &EventRef,
    ) -> Result<hh_ledger::event::SeqRange, SpawnError> {
        result::detach_to_child(
            store,
            &binding.parent_run_id,
            &binding.parent_lease,
            child_run_id,
            decision,
        )
    }

    /// Parent-stop — C-2's drain (every live `cancel` child gets
    /// `cancelled{parent_stop}`; T6 children detach).
    pub fn on_parent_stop(
        store: &mut Store,
        binding: &ParentBinding,
    ) -> Result<Vec<String>, SpawnError> {
        result::drain_on_parent_stop(store, &binding.parent_run_id, &binding.parent_lease)
    }

    /// Parent-restore — C-3/C-4's adoption pass.
    pub fn on_parent_restore(
        store: &mut Store,
        binding: &ParentBinding,
    ) -> Result<hh_subagent::recovery::RecoveryReport, SpawnError> {
        hh_subagent::recovery::recover_parent(store, &binding.parent_run_id, &binding.parent_lease)
    }

    /// Parent-mediated messaging — the relay scan: every `sent` row in
    /// the tree addressed to `receiver` becomes an occurrence on its
    /// `peer_message{from}` subscription (under the receiver's lease).
    pub fn relay(
        store: &mut Store,
        receiver_run_id: &str,
        receiver_lease: &Lease,
        tree_run_ids: &[String],
    ) -> Result<Vec<String>, SpawnError> {
        messaging::inbox_scan(store, receiver_run_id, receiver_lease, tree_run_ids)
    }

    /// The tree's relay set (parent + direct children — tree-local reach).
    pub fn relay_set(store: &Store, binding: &ParentBinding) -> Result<Vec<String>, SpawnError> {
        messaging::relay_set(store, &binding.parent_run_id)
    }
}

/// `Spawned` minus the lease — the orchestrator's fold value (the lease is
/// the caller's; a run record never holds it).
pub trait SpawnedShallow {
    /// A lease-free copy.
    fn clone_shallow(&self) -> Spawned;
}

impl SpawnedShallow for Spawned {
    fn clone_shallow(&self) -> Spawned {
        Spawned {
            child_run_id: self.child_run_id.clone(),
            child_handles: self.child_handles.clone(),
            budget_id: self.budget_id.clone(),
            env_handle_id: self.env_handle_id.clone(),
            spawn_event: self.spawn_event.clone(),
            child_lease: None,
            wait_mode: self.wait_mode,
            subscription_id: self.subscription_id.clone(),
        }
    }
}
