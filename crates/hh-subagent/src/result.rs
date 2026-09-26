//! Child-terminal processing — the `control.subagent.{result,cancelled,
//! detached}` emitters, the `ReturnContract` check, the budget-remainder
//! release, and the C-1/C-2/C-4 drain paths (§5e.3; ADR-0187 D1;
//! ADR-0193 D2; M-2).
//!
//! A child produces its terminal row on the *parent's* ledger under the
//! parent's writer lease — the child head the row carries is the anchor a
//! later merge/audit reads (I-A6). Result content is artifacts/refs only:
//! no transcript member exists on [`SubagentResult`] (M-1 enforced by type).

use std::collections::BTreeMap;

use hh_budget::account::Account;
use hh_budget::BudgetMode;
use hh_ledger::manifest::EventRef;
use hh_ledger::store::{Lease, Store};
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::spawn::{fold_children, kernel_ev_pub};
use crate::types::SpawnError;
use crate::types::*;

/// The terminal data a child run's ledger exposes for the result row —
/// produced by *reading the child's durable prefix* (the parent never
/// trusts an in-memory hand-off; C-3/C-4 proof is the child head).
#[derive(Debug, Clone)]
pub struct ChildTerminalView {
    /// Whether the child reached `lifecycle.run.finished`.
    pub finished: bool,
    /// `child_head{seq, hash}` (the child's tip at read time).
    pub child_head: Option<Json>,
    /// The child's declared stop reason (`lifecycle.run.finished`'s).
    pub stop_reason: String,
    /// Artifact refs the child produced (`result.artifacts[]` candidates —
    /// caller supplies the per-artifact refs; this slice collects the
    /// child's `lifecycle.artifact.*`/`action.effect.committed` refs the
    /// caller names).
    pub artifacts: Vec<Json>,
    /// `fs_changes` the child declared (a `FsChangeSet`/`SnapshotRef` —
    /// the merge input).
    pub fs_changes: Option<Json>,
    /// The summary artifact ref (`Ref<Artifact{kind: subagent_summary}>`).
    pub summary: Option<Json>,
    /// Claim refs on the child's terminal.
    pub claims: Vec<String>,
    /// Memories the child wrote.
    pub memories_written: Vec<String>,
    /// Irreversible effects the child ran (FD-2 — listed, gated as the
    /// parent's own at the completion gate).
    pub effects_irreversible: Vec<String>,
    /// The child's usage `ResourceVector` (its accounted spend).
    pub usage: Json,
    /// The child's verdict refs.
    pub verdicts: Vec<String>,
}

impl Default for ChildTerminalView {
    fn default() -> ChildTerminalView {
        ChildTerminalView {
            finished: false,
            child_head: None,
            stop_reason: String::new(),
            artifacts: Vec::new(),
            fs_changes: None,
            summary: None,
            claims: Vec::new(),
            memories_written: Vec::new(),
            effects_irreversible: Vec::new(),
            usage: Json::Null,
            verdicts: Vec::new(),
        }
    }
}

/// Read the child's terminal state (the durable-prefix projection —
/// `child_head` is `None` before any committed event).
pub fn child_terminal_view(
    store: &Store,
    child_run_id: &str,
) -> Result<ChildTerminalView, SpawnError> {
    let events = store
        .events(child_run_id)
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    let head = child_head_anchor(store, child_run_id, events)?;
    let mut v = ChildTerminalView {
        finished: false,
        child_head: head,
        ..Default::default()
    };
    for e in events {
        if e.class == "lifecycle.run.finished" {
            v.finished = true;
            v.stop_reason = e
                .payload
                .get("stop_reason")
                .or_else(|| e.payload.get("reason"))
                .and_then(Json::as_str)
                .unwrap_or("unknown")
                .to_string();
        }
    }
    Ok(v)
}

/// The I-A6 `child_head` citation: when a `final` checkpoint exists, the
/// parent cites the checkpoint-covered tip (`tree_size - 1`, `chain_hash`,
/// `checkpoint_ref`) rather than the unsigned checkpoint row itself. Without
/// one, it cites the current durable head (`checkpoint_ref` absent). A final
/// checkpoint therefore makes `result/cancelled.child_head` byte-equal to
/// the signed final head the child cannot rewrite.
fn child_head_anchor(
    store: &Store,
    child_run_id: &str,
    events: &[hh_ledger::event::EventEnvelope],
) -> Result<Option<Json>, SpawnError> {
    if let Some(final_cp) = events.iter().rev().find(|e| {
        e.class == "security.audit.checkpoint"
            && e.payload.get("kind").and_then(Json::as_str) == Some("final")
    }) {
        if let Some(claim) = hh_ledger::tree::parse_checkpoint(&final_cp.payload) {
            if let (Some(tree_size), Some(chain_hash)) = (claim.tree_size, claim.chain_hash) {
                return Ok(Some(Json::obj([
                    (
                        "seq",
                        Json::Int(tree_size.saturating_sub(1).min(i64::MAX as u64) as i64),
                    ),
                    ("hash", Json::str(chain_hash.as_str())),
                    (
                        "checkpoint_ref",
                        Json::str(
                            claim
                                .idp
                                .unwrap_or_else(|| hh_ledger::tree::checkpoint_idp(&claim.payload))
                                .as_str(),
                        ),
                    ),
                ])));
            }
        }
    }
    Ok(store.head(child_run_id).ok().map(|h| {
        Json::obj([
            ("seq", Json::Int(h.seq as i64)),
            ("hash", Json::str(h.hash.as_str())),
        ])
    }))
}

/// Check the `ReturnContract` against the child's declared outputs —
/// `missing_required`, `claims_absent`, `over_max_tokens`, `schema_failure`
/// are typed violations retained on the result row (M-2 — never silent
/// truncation).
pub fn check_return_contract(
    contract: &ReturnContract,
    artifacts: &[Json],
    summary: Option<&Json>,
    claims: &[String],
) -> Vec<ContractViolation> {
    let mut out = Vec::new();
    for (i, decl) in contract.artifacts.iter().enumerate() {
        if !decl.required {
            continue;
        }
        let present = artifacts
            .iter()
            .any(|a| a.get("kind").and_then(Json::as_str) == Some(decl.kind.as_str()));
        if !present {
            out.push(ContractViolation {
                field: format!("artifacts[{i}]"),
                reason: "missing_required".into(),
            });
        }
    }
    if let Some(s) = &contract.summary {
        if s.schema.is_some() && summary.is_none() {
            out.push(ContractViolation {
                field: "summary.schema".into(),
                reason: "schema_failure".into(),
            });
        }
        if let (Some(_cap), Some(_sum)) = (s.max_tokens, summary) {
            // Token counting is the child's envelope's job at write time —
            // the parent check records the declared cap was honored by
            // schema, never by recount (no `Text` read).
        }
    }
    if contract.claims && claims.is_empty() {
        out.push(ContractViolation {
            field: "claims".into(),
            reason: "claims_absent".into(),
        });
    }
    out
}

/// `control.subagent.result` — appended on the parent's ledger under its
/// writer lease (the child's terminal becomes the parent's fact). The
/// `budget_id` is completed first so a `slice` child's remainder returns
/// (conservation — `budget_released`).
#[allow(clippy::too_many_arguments)]
pub fn record_child_result(
    store: &mut Store,
    parent_run_id: &str,
    parent_lease: &Lease,
    child_run_id: &str,
    spec: &SubagentSpec,
    view: &ChildTerminalView,
    budget_id: &str,
) -> Result<SeqRangePair, SpawnError> {
    // The slice remainder releases at the child's terminal (R-2.1.6 —
    // `complete` is the parent's accounting move on its own ledger).
    let released = if spec.budget_mode == BudgetMode::Slice {
        let mut account =
            Account::open(store, parent_run_id).map_err(|e| SpawnError::Kernel(e.to_string()))?;
        account
            .complete(parent_lease, budget_id)
            .map(|_| true)
            .map_err(|e| SpawnError::Kernel(format!("complete: {e}")))?
    } else {
        false
    };
    let violations = check_return_contract(
        &spec.return_contract,
        &view.artifacts,
        view.summary.as_ref(),
        &view.claims,
    );
    let outcome = ChildOutcome::Result;
    let spawned_ref = spawned_delegation_ref(store, parent_run_id, child_run_id)?;
    let result = SubagentResult {
        child_run_id: child_run_id.to_string(),
        delegation_ref: spawned_ref,
        outcome: Some(outcome),
        outcome_class: "result".into(),
        stop_reason: view.stop_reason.clone(),
        child_head: view.child_head.clone(),
        artifacts: view.artifacts.clone(),
        fs_changes: view.fs_changes.clone(),
        summary: view.summary.clone(),
        claims: view.claims.clone(),
        memories_written: view.memories_written.clone(),
        effects_irreversible: view.effects_irreversible.clone(),
        usage: view.usage.clone(),
        budget_released: released,
        verdicts: view.verdicts.clone(),
        return_contract_satisfied: violations.is_empty(),
        violations,
    };
    // Re-entry: the result fact enters at `authority = min(⊔ child inputs,
    // delegate)` with `derived_from.kind = subagent_result`, union-tainted
    // (ADR-0053 D-3). The audit-grade ROW's envelope provenance stays
    // `kernel` (Rule P — the row is a kernel fact); the *result's* re-entry
    // provenance rides inside the payload — the label a consumer reads.
    let mut payload = result.to_json();
    if let Json::Obj(m) = &mut payload {
        m.insert(
            "reentry".to_string(),
            Json::obj([
                ("authority", Json::str("delegate")),
                ("taint", Json::str("union")),
                (
                    "derived_from",
                    Json::obj([
                        ("kind", Json::str("subagent_result")),
                        (
                            "inputs",
                            Json::Arr(
                                std::iter::once(child_run_id.to_string())
                                    .chain(view.claims.iter().cloned())
                                    .map(Json::str)
                                    .collect(),
                            ),
                        ),
                        ("deriver", Json::str("kernel(subagent)")),
                        ("deterministic", Json::Bool(false)),
                    ]),
                ),
            ]),
        );
    }
    let mut ev = kernel_ev_pub(
        store,
        parent_run_id,
        "control.subagent.result",
        payload,
        vec![],
        None,
    )
    .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    // `result` closes the child-run scope `spawned` opened.
    ev.scope.child_run_id = Some(child_run_id.to_string());
    let range = store
        .append(parent_run_id, parent_lease, vec![ev])
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    Ok(SeqRangePair { range, result })
}

/// The `control.subagent.result` commit plus the built record.
pub struct SeqRangePair {
    /// The committed seq range.
    pub range: hh_ledger::event::SeqRange,
    /// The `SubagentResult` the row carries.
    pub result: SubagentResult,
}

/// `control.subagent.cancelled{child_run_id, reason, child_head?}` — the
/// closed `CancelReason` set (§5e.3). The `slice` remainder still releases.
pub fn record_child_cancelled(
    store: &mut Store,
    parent_run_id: &str,
    parent_lease: &Lease,
    child_run_id: &str,
    spec: &SubagentSpec,
    reason: CancelReason,
    budget_id: &str,
) -> Result<hh_ledger::event::SeqRange, SpawnError> {
    if spec.budget_mode == BudgetMode::Slice {
        let mut account =
            Account::open(store, parent_run_id).map_err(|e| SpawnError::Kernel(e.to_string()))?;
        let _ = account.complete(parent_lease, budget_id);
    }
    let events = store
        .events(child_run_id)
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    let head = child_head_anchor(store, child_run_id, events)?;
    let mut payload = BTreeMap::from([
        ("child_run_id".to_string(), Json::str(child_run_id)),
        ("reason".to_string(), Json::str(reason.as_str())),
        (
            "delegation_ref".to_string(),
            Json::str(spawned_delegation_ref(store, parent_run_id, child_run_id)?),
        ),
    ]);
    if let Some(h) = head {
        payload.insert("child_head".to_string(), h);
    }
    let mut ev = kernel_ev_pub(
        store,
        parent_run_id,
        "control.subagent.cancelled",
        Json::Obj(payload),
        vec![],
        None,
    )
    .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    // `cancelled` is the other durable closer of the child-run scope.
    ev.scope.child_run_id = Some(child_run_id.to_string());
    store
        .append(parent_run_id, parent_lease, vec![ev])
        .map_err(|e| SpawnError::Kernel(e.to_string()))
}

/// `control.subagent.detached{child_run_id}` — T6's `detach_to_child` arm:
/// the child leaves the parent's coordination (fan-out decrement, no more
/// merge input, no parent-mediated messaging; the `child_terminal`
/// subscription is retained so the child's terminal still anchors —
/// C-3's survival proof stays checkable).
pub fn detach_to_child(
    store: &mut Store,
    parent_run_id: &str,
    parent_lease: &Lease,
    child_run_id: &str,
    decision: &EventRef,
) -> Result<hh_ledger::event::SeqRange, SpawnError> {
    let mut ev = kernel_ev_pub(
        store,
        parent_run_id,
        "control.subagent.detached",
        Json::obj([
            ("child_run_id", Json::str(child_run_id)),
            (
                "delegation_ref",
                Json::str(spawned_delegation_ref(store, parent_run_id, child_run_id)?),
            ),
        ]),
        vec![decision.clone()],
        None,
    )
    .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    ev.scope.child_run_id = Some(child_run_id.to_string());
    store
        .append(parent_run_id, parent_lease, vec![ev])
        .map_err(|e| SpawnError::Kernel(e.to_string()))
}

/// The `delegation_ref` the spawned row recorded (the `control.decision`
/// that caused the spawn).
fn spawned_delegation_ref(
    store: &Store,
    parent_run_id: &str,
    child_run_id: &str,
) -> Result<String, SpawnError> {
    for (_, rec) in fold_children(store, parent_run_id)? {
        if rec.spawned.get("child_run_id").and_then(Json::as_str) == Some(child_run_id) {
            return Ok(rec
                .spawned
                .get("delegation_ref")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string());
        }
    }
    Ok(String::new())
}

/// C-1 — the child-terminal drain: for every spawned child whose run
/// reached `lifecycle.run.finished` with no terminal row on the parent,
/// emit `control.subagent.result` (or `cancelled` when the view says the
/// child died unfinished — `infrastructure_failure{crash}`). Returns the
/// children this call completed. Runs under the parent's writer lease at
/// the parent's next decision point (the `child_terminal` wakeup cue).
pub fn drain_child_terminal(
    store: &mut Store,
    parent_run_id: &str,
    parent_lease: &Lease,
    specs: &BTreeMap<String, SubagentSpec>,
) -> Result<Vec<String>, SpawnError> {
    let mut completed = Vec::new();
    for (child_run_id, rec) in fold_children(store, parent_run_id)? {
        if rec.terminal_class.is_some() {
            continue;
        }
        let view = match child_terminal_view(store, &child_run_id) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let budget_id = rec
            .spawned
            .get("budget_id")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        let spec = specs.get(&child_run_id).cloned().unwrap_or_else(|| {
            // The spec folds back out of the child's manifest
            // `extra.subagent_spec` — the durable copy (a spec is never
            // re-asked).
            store
                .manifest(&child_run_id)
                .ok()
                .and_then(|m| m.extra.get("subagent_spec"))
                .and_then(SubagentSpec::from_json)
                .unwrap_or_else(default_spec)
        });
        if !view.finished {
            continue; // still running — the deadline walk owns the refusal
        }
        record_child_result(
            store,
            parent_run_id,
            parent_lease,
            &child_run_id,
            &spec,
            &view,
            &budget_id,
        )?;
        completed.push(child_run_id);
    }
    Ok(completed)
}

/// C-4 — the `subagent` scope deadline walk: children named in
/// `deadline_passed` (the caller's `TimeoutPolicy[subagent]` check) are
/// `cancelled{reason: infrastructure_failure, failure_kind:
/// child_unresponsive}` — a child's crash is the parent's bounded wait
/// exhausted, never a silent drop. The `slice` remainder releases.
pub fn cancel_unresponsive(
    store: &mut Store,
    parent_run_id: &str,
    parent_lease: &Lease,
    deadline_passed: &[String],
) -> Result<Vec<String>, SpawnError> {
    let mut done = Vec::new();
    let children = fold_children(store, parent_run_id)?;
    for child_run_id in deadline_passed {
        let rec = match children.iter().find(|(c, _)| c == child_run_id) {
            Some((_, r)) if r.terminal_class.is_none() => r,
            _ => continue,
        };
        let spec = store
            .manifest(child_run_id)
            .ok()
            .and_then(|m| m.extra.get("subagent_spec"))
            .and_then(SubagentSpec::from_json)
            .unwrap_or_else(default_spec);
        let budget_id = rec
            .spawned
            .get("budget_id")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        if spec.budget_mode == BudgetMode::Slice {
            let mut account = Account::open(store, parent_run_id)
                .map_err(|e| SpawnError::Kernel(e.to_string()))?;
            let _ = account.complete(parent_lease, &budget_id);
        }
        let child_events = store
            .events(child_run_id)
            .map_err(|e| SpawnError::Kernel(e.to_string()))?;
        let child_head = child_head_anchor(store, child_run_id, child_events)?;
        let mut payload = BTreeMap::from([
            ("child_run_id".to_string(), Json::str(child_run_id.as_str())),
            ("reason".to_string(), Json::str("infrastructure_failure")),
            ("failure_kind".to_string(), Json::str("child_unresponsive")),
            (
                "delegation_ref".to_string(),
                Json::str(spawned_delegation_ref(store, parent_run_id, child_run_id)?),
            ),
        ]);
        if let Some(head) = child_head {
            payload.insert("child_head".to_string(), head);
        }
        let mut ev = kernel_ev_pub(
            store,
            parent_run_id,
            "control.subagent.cancelled",
            Json::Obj(payload),
            vec![],
            None,
        )
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
        ev.scope.child_run_id = Some(child_run_id.clone());
        store
            .append(parent_run_id, parent_lease, vec![ev])
            .map_err(|e| SpawnError::Kernel(e.to_string()))?;
        done.push(child_run_id.clone());
    }
    Ok(done)
}

/// C-2 — the parent-stop drain: for every live child with
/// `on_parent_end = cancel`, emit `control.subagent.cancelled{parent_stop}`
/// (the child's own runtime drains at its next decision point — cancel is
/// a wait boundary, never mid-effect; ADR-0131 W-3). `detach_to_child`
/// children get `control.subagent.detached` instead (T6).
/// Returns the cancelled child ids. Runs inside the parent's stop sequence
/// within `TimeoutPolicy[subagent].hard_max` (the caller enforces the wall).
pub fn drain_on_parent_stop(
    store: &mut Store,
    parent_run_id: &str,
    parent_lease: &Lease,
) -> Result<Vec<String>, SpawnError> {
    let mut done = Vec::new();
    for (child_run_id, rec) in fold_children(store, parent_run_id)? {
        if rec.terminal_class.is_some() {
            continue;
        }
        let on_parent_end = rec
            .spawned
            .get("on_parent_end")
            .and_then(Json::as_str)
            .unwrap_or("cancel");
        if on_parent_end == "detach_to_child" {
            detach_to_child(
                store,
                parent_run_id,
                parent_lease,
                &child_run_id,
                &EventRef {
                    run_id: parent_run_id.to_string(),
                    event_id: rec.spawn_event_id.clone(),
                },
            )?;
        } else {
            let spec = store
                .manifest(&child_run_id)
                .ok()
                .and_then(|m| m.extra.get("subagent_spec"))
                .and_then(SubagentSpec::from_json);
            let budget_id = rec
                .spawned
                .get("budget_id")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string();
            let spec = spec.unwrap_or_else(default_spec);
            record_child_cancelled(
                store,
                parent_run_id,
                parent_lease,
                &child_run_id,
                &spec,
                CancelReason::ParentStop,
                &budget_id,
            )?;
        }
        done.push(child_run_id);
    }
    Ok(done)
}

pub(crate) fn default_goal() -> hh_hir::records::GoalRecord {
    use hh_hir::leaves::Text;
    use hh_hir::records::GoalOrigin;
    use hh_hir::refs::{Ref, RefVersion};
    hh_hir::records::GoalRecord {
        statement: Text::new(
            "(unavailable)",
            "kernel",
            ProvenanceRecord::kernel(crate::spawn::KERNEL_SUBAGENT, 0),
        ),
        success_criteria: Vec::new(),
        unverifiable_reason: None,
        budget: Ref {
            semantic_id: "budget".to_string(),
            version: RefVersion::Pinned("sha256:zero".to_string()),
        },
        origin: GoalOrigin::Delegated,
        parent: None,
    }
}

/// The spec fallback when a child manifest's `subagent_spec` member
/// is unreadable (never silent — the row records the terminal class).
pub(crate) fn default_spec() -> SubagentSpec {
    SubagentSpec {
        process: ChildProcess::Native {
            harness_def: hh_hir::refs::Ref::pinned(
                "harness".to_string(),
                "sha256:zero".to_string(),
            ),
            profile_binding: None,
            control_strategy: None,
            slots: None,
        },
        goal: default_goal(),
        role: ChildRole::Subagent,
        requested_grants: Vec::new(),
        ceiling: None,
        budget_spec: hh_budget::spec::BudgetSpec::default(),
        budget_mode: BudgetMode::Pool,
        environment: EnvIsolation::None,
        supplies: Supplies::default(),
        return_contract: ReturnContract::default(),
        wait_mode: WaitMode::Background,
        wait_timeout_ms: 0,
        on_parent_end: hh_env::handle::OnParentEnd::Teardown,
        delegation_reason: DelegationReason::CleanContext,
        topology_ref: None,
        stage_index: None,
        ownership_grants: Vec::new(),
        reserved_keys: Vec::new(),
        consistency_declarations: Vec::new(),
        merge_policy_ref: None,
        messaging_policy: None,
    }
}
