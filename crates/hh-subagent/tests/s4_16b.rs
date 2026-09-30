//! S4.16b acceptance — AC-R-2.4.5-8 (spec §5c.5 `subagent_task`; ticket
//! S4.16b): the `spec_for_procedure` DelegateSpec constructor assembles the
//! canonical `SubagentSpec` the compiler's `delegate` node carries
//! (`spec.to_json()` is the node's `spec` member; `SubagentSpec::from_json`
//! is the interpreter's recovery — the JSON is the one schema), `spawn`
//! consumes it with Π's attenuation, and the child's return re-enters at
//! `authority ≤ delegate` with `derived_from.kind = subagent_result`.

use hh_budget::{Account, BudgetMode, BudgetScope, BudgetScopeKind, BudgetSpec};
use hh_compiler::plan::PinnedRef;
use hh_env::handle::OnParentEnd;
use hh_hir::kinds::{EffectClass, EffectDomain};
use hh_hir::records::{Grant, GrantConstraints};
use hh_hir::refs::Ref;
use hh_ledger::audit::{CheckpointKind, FixedSigner};
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::manifest::{EventRef, RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_monitor::decision::DecisionScope;
use hh_monitor::delegate::LiveCoords;
use hh_monitor::handle::{AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis};
use hh_monitor::table::HandleTable;
use hh_ontology::control::Owner;
use hh_ontology::dimensions::{DimensionId, DimensionKey};
use hh_provenance::{AuthorityClass, ProvenanceRecord};
use hh_subagent::ownership::OwnershipTable;
use hh_subagent::result::{child_terminal_view, record_child_result};
use hh_subagent::spawn::{spawn, SpawnCtx};
use hh_subagent::task::{spec_for_procedure, TaskSpecError, TaskSpecInputs};
use hh_subagent::types::*;
use hh_wire::json::Json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

// ── fixtures (the same real-ledger scaffold the S4.6 suite runs spawn over) ──

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-subagent-s416b-{tag}-{n}"));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn kernel_ev(store: &Store, run_id: &str, class: &str, payload: Json) -> Event {
    Event {
        event_id: store.alloc_id("evt"),
        class: class.to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel("kernel:test"),
        scope: Scope::default(),
        parent_event_id: store.head_event_id(run_id).unwrap(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel("kernel:test", store.now_ms())),
        content_kind: None,
        payload,
    }
}

struct Parent {
    store: Store,
    run_id: String,
    lease: Lease,
    budget_id: String,
    decision: EventRef,
    handle_id: HandleId,
    handles: HandleTable,
    ownerships: OwnershipTable,
    tools: Vec<Ref>,
}

fn grant(domain: EffectDomain, scope: &str, delegable: bool) -> Grant {
    Grant {
        effect: EffectClass::domain_only(domain),
        scope: scope.to_string(),
        constraints: GrantConstraints {
            budget: None,
            time: None,
            count: None,
        },
        delegable,
    }
}

fn open_parent(tag: &str) -> Parent {
    let mut store = Store::open_test(dir(tag), 1_000).unwrap();
    let mut manifest = RunManifest::minimal(RunKind::Agent);
    manifest.signer_key_ids = vec!["test-key".to_string()];
    let (run_id, lease) = store.open_run(manifest, "writer-parent").unwrap();
    let mut p = Parent {
        store,
        run_id: run_id.clone(),
        lease,
        budget_id: String::new(),
        decision: EventRef {
            run_id,
            event_id: String::new(),
        },
        handle_id: HandleId("hnd-parent".to_string()),
        handles: HandleTable::default(),
        ownerships: OwnershipTable::default(),
        tools: Vec::new(),
    };
    // The delegate decision row — the idempotency key's decision half.
    let ev = kernel_ev(
        &p.store,
        &p.run_id,
        "control.decision",
        Json::obj([
            ("kind", Json::str("delegate")),
            ("delegation_reason", Json::str("specialization")),
            ("decision_id", Json::str("dec-task")),
        ]),
    );
    let id = ev.event_id.clone();
    p.store.append(&p.run_id, &p.lease, vec![ev]).unwrap();
    p.decision = EventRef {
        run_id: p.run_id.clone(),
        event_id: id,
    };
    let mut account = Account::open(&mut p.store, &p.run_id).unwrap();
    p.budget_id = account
        .allocate(
            &p.lease,
            None,
            BudgetScope {
                kind: BudgetScopeKind::AgentProcess,
                target: p.run_id.clone(),
            },
            BudgetSpec::hard_caps(
                BudgetMode::Pool,
                &[
                    (DimensionKey::Primary(DimensionId::ModelCalls), 1_000),
                    (DimensionKey::Primary(DimensionId::Spawns), 8),
                    (DimensionKey::Primary(DimensionId::FanOut), 8),
                    (DimensionKey::Primary(DimensionId::DelegationDepth), 4),
                ],
            ),
        )
        .unwrap();
    let handle = AuthorityHandle {
        handle_id: p.handle_id.clone(),
        permission_ref: PinnedRef {
            semantic_id: "perm:root".to_string(),
            version_id: "v1".to_string(),
        },
        holder: Ref::pinned("agent_process", p.run_id.clone()),
        issuer: ProvenanceRecord::kernel("kernel:test", 0),
        grants: vec![
            grant(EffectDomain::FsRead, "*", true),
            grant(EffectDomain::FsWrite, "*", true),
            grant(EffectDomain::ModelCall, "*", true),
            grant(EffectDomain::SpawnProcess, "*", true),
        ],
        ceiling: AuthorityClass::Delegate,
        validity: HandleValidity {
            issued_at: "evt-mint".to_string(),
            expires_at: Some(HandleExpiry::Run(p.run_id.clone())),
            revoked_by: None,
        },
        parent_handle: None,
        delegable: true,
        origin_basis: OriginBasis::Seal,
        basis_ref: "def:test#v1".to_string(),
        budget_ref: None,
        scope: DecisionScope::Session,
    };
    p.handles.handles.insert(p.handle_id.clone(), handle);
    p.ownerships = OwnershipTable::project(
        &p.store,
        &[],
        vec![(p.run_id.clone(), OwnedObject::FsPathPrefix("/".to_string()))],
    )
    .unwrap();
    p
}

fn ctx<'a>(p: &'a mut Parent) -> SpawnCtx<'a> {
    let run_id: &'a str = &p.run_id;
    SpawnCtx {
        store: &mut p.store,
        handles: &p.handles,
        parent_run_id: run_id,
        parent_lease: &p.lease,
        parent_handle_id: &p.handle_id,
        parent_budget_id: &p.budget_id,
        parent_env_id: None,
        env_driver: None,
        parent_ownerships: &p.ownerships,
        parent_tool_table: &p.tools,
        parent_depth: 0,
        decision: p.decision.clone(),
        owner: Owner::Code,
        holder: "writer-child",
        coords: LiveCoords {
            effect_id: "eff-0".to_string(),
            turn_id: "turn-0".to_string(),
            run_id: run_id.to_string(),
            session: String::new(),
            seq: 0,
        },
        reserve_ttl_ms: 30_000,
        branch_ctx: None,
        hosted_plane: None,
        hook: None,
    }
}

/// The `subagent_task` spec inputs — a `read_file`-shaped procedure: one
/// declared `fs_read` effect, one delegated `fs_read` grant covering it.
fn task_inputs() -> TaskSpecInputs {
    TaskSpecInputs {
        child_definition: Ref::pinned("def:child", format!("sha256:{}", "d".repeat(64))),
        procedure: Ref::pinned("proc:read_file", "v1"),
        parameters: Json::obj([("path", Json::str("/src/main.rs"))]),
        allowed_effects: vec![EffectClass::domain_only(EffectDomain::FsRead)],
        requested_grants: vec![grant(EffectDomain::FsRead, "/src/**", true)],
        budget_spec: BudgetSpec::hard_caps(
            BudgetMode::Slice,
            &[
                (DimensionKey::Primary(DimensionId::ModelCalls), 10),
                (DimensionKey::Primary(DimensionId::Spawns), 0),
                (DimensionKey::Primary(DimensionId::FanOut), 0),
                (DimensionKey::Primary(DimensionId::DelegationDepth), 0),
            ],
        ),
        budget_ref: Ref::pinned("budget:child", "v1"),
        parent_goal: Some(Ref::pinned("goal:parent", "v1")),
        wait_mode: WaitMode::Await,
        wait_timeout_ms: 60_000,
        return_contract: ReturnContract::default(),
        on_parent_end: OnParentEnd::Teardown,
        environment: EnvIsolation::None,
    }
}

// ── AC-R-2.4.5-8 — spec → spawn → re-entry ───────────────────────────────────

/// The constructor assembles the DelegateSpec: ceiling `delegate`, fresh
/// context, `supplies.procedures` carrying the pinned procedure ref, a
/// delegated goal naming the task + bound parameters, slice budget ⊆ the
/// parent's pool, and the spec refuses an uncovered capability effect.
#[test]
fn ac_r_2_4_5_8_spec_for_procedure_shape() {
    let inputs = task_inputs();
    let spec = spec_for_procedure(&inputs, 0).expect("covered effects");
    // Result ceiling — §5c.5's `≤ delegate` obligation, baked at spec mint.
    assert_eq!(spec.ceiling, Some(AuthorityClass::Delegate));
    // Fresh context + child memory namespace (the canonical isolation row).
    assert_eq!(spec.context, ContextIsolation::Fresh);
    assert!(matches!(spec.process, ChildProcess::Native { .. }));
    // The pinned procedure ref travels in supplies (SP-3's only channel)
    // and as the goal's success criterion.
    assert_eq!(spec.supplies.procedures, vec![inputs.procedure.clone()]);
    assert_eq!(spec.goal.success_criteria, vec![inputs.procedure.clone()]);
    assert_eq!(spec.goal.origin, hh_hir::records::GoalOrigin::Delegated);
    assert_eq!(spec.goal.parent, inputs.parent_goal);
    // The statement names the task + bound params — never procedure prose.
    let stmt = spec.goal.statement.content.as_deref().unwrap();
    assert!(stmt.contains("proc:read_file"), "task names the procedure");
    assert!(stmt.contains("/src/main.rs"), "task carries bound params");
    assert_eq!(spec.delegation_reason, DelegationReason::Specialization);
    assert_eq!(spec.budget_mode, BudgetMode::Slice);

    // Π ⊆ is a guard: an uncovered declared effect refuses typed.
    let mut bad = task_inputs();
    bad.allowed_effects
        .push(EffectClass::domain_only(EffectDomain::FsWrite));
    assert!(matches!(
        spec_for_procedure(&bad, 0),
        Err(TaskSpecError::UncoveredCapability { .. })
    ));
}

/// The DelegateSpec is data — `spec.to_json()` is exactly the `spec` member
/// the compiler's `delegate` plan node carries, and
/// `SubagentSpec::from_json` recovers the identical record for `spawn`.
#[test]
fn ac_r_2_4_5_8_spec_json_round_trip_is_the_delegate_node_seam() {
    let spec = spec_for_procedure(&task_inputs(), 0).unwrap();
    let j = spec.to_json();
    // The isolation row stamps the fresh-context/child-namespace contract.
    let iso = j.get("isolation").unwrap();
    assert_eq!(iso.get("context").and_then(Json::as_str), Some("fresh"));
    assert_eq!(
        iso.get("memory_namespace").and_then(Json::as_str),
        Some("child")
    );
    let recovered = SubagentSpec::from_json(&j).expect("canonical parse");
    // The statement leaf re-enters content-addressed — `content` stays out
    // of the canonical form (SP-3's no-Text-read rule); the hash is the
    // identity and the rest of the record is byte-equal.
    assert!(recovered.goal.statement.content.is_none());
    assert_eq!(
        recovered.goal.statement.content_hash, spec.goal.statement.content_hash,
        "content-addressed — same leaf identity"
    );
    let mut addressed = spec.clone();
    addressed.goal.statement.content = None;
    assert_eq!(
        recovered, addressed,
        "one schema source — parse is the inverse"
    );
}

/// End-to-end: spec → `spawn` (attenuation to `delegable(parent)`, fresh
/// context row) → child terminal → `record_child_result` → the durable
/// `control.subagent.result` row re-enters at `delegate` with
/// `derived_from.kind = subagent_result`.
#[test]
fn ac_r_2_4_5_8_spawn_and_reentry() {
    let mut p = open_parent("ac-r2458");
    let spec = spec_for_procedure(&task_inputs(), 0).unwrap();
    // The DelegateNode seam: spawn consumes the canonical spec — exactly
    // what `SubagentSpec::from_json(delegate_node.spec)` yields.
    let spec = SubagentSpec::from_json(&spec.to_json()).unwrap();
    let spawned = {
        let mut c = ctx(&mut p);
        spawn(&mut c, &spec).expect("lawful spec spawns")
    };
    assert!(spawned.child_run_id.starts_with("sub-"));

    // The spawned row stamps the attenuated ceiling — the child's writes
    // can never exceed `delegate`.
    let row = p
        .store
        .events(&p.run_id)
        .unwrap()
        .iter()
        .find(|e| e.class == "control.subagent.spawned")
        .expect("spawned row durable")
        .clone();
    assert_eq!(
        row.payload.get("ceiling").and_then(Json::as_str),
        Some("delegate"),
        "child ceiling is delegate — result re-entry can never exceed it"
    );

    // The child finishes; the result row carries the re-entry provenance.
    let child_lease = spawned.child_lease.clone().unwrap();
    let fin = kernel_ev(
        &p.store,
        &spawned.child_run_id,
        "lifecycle.run.finished",
        Json::obj([("stop_reason", Json::str("goal_achieved"))]),
    );
    p.store
        .append(&spawned.child_run_id, &child_lease, vec![fin])
        .unwrap();
    p.store
        .checkpoint(
            &spawned.child_run_id,
            &child_lease,
            CheckpointKind::Final,
            &mut FixedSigner::new("test-key", b"s4.16b-test-signing-key".to_vec()),
        )
        .unwrap();
    let mut view = child_terminal_view(&p.store, &spawned.child_run_id).unwrap();
    assert!(view.finished);
    view.artifacts = vec![Json::str("art:task-out")];
    let pair = record_child_result(
        &mut p.store,
        &p.run_id,
        &p.lease,
        &spawned.child_run_id,
        &spec,
        &view,
        &spawned.budget_id,
    )
    .unwrap();
    assert_eq!(pair.result.outcome_class, "result");

    let result_row = p
        .store
        .events(&p.run_id)
        .unwrap()
        .iter()
        .find(|e| e.class == "control.subagent.result")
        .expect("result row durable")
        .clone();
    let reentry = result_row.payload.get("reentry").expect("reentry member");
    assert_eq!(
        reentry.get("authority").and_then(Json::as_str),
        Some("delegate"),
        "result authority ≤ delegate"
    );
    assert_eq!(
        reentry
            .get("derived_from")
            .and_then(|d| d.get("kind"))
            .and_then(Json::as_str),
        Some("subagent_result"),
        "derived_from.kind = subagent_result"
    );
}
