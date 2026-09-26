//! S4.6 acceptance matrix — AC-R-2.6.3-1…12 + AC-R-2.1.6-4 + KP-14/16–21
//! fault-injection over the real `hh-ledger` store (no mocks at the ledger
//! seam — the spawn path is exercised end-to-end).

use hh_budget::{Account, BudgetMode, BudgetScope, BudgetScopeKind, BudgetSpec};
use hh_compiler::plan::PinnedRef;
use hh_env::handle::{DeriveMode, OnParentEnd};
use hh_hir::kinds::{EffectClass, EffectDomain};
use hh_hir::leaves::Text;
use hh_hir::records::{GoalOrigin, GoalRecord, Grant, GrantConstraints};
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
use hh_subagent::merge::{merge, project_merge_report, MergeCtx, MergeInput, MergeVeto};
use hh_subagent::messaging::{inbox_scan, relay_set, send_message, SendCtx};
use hh_subagent::ownership::OwnershipTable;
use hh_subagent::recovery::{cancel_on_revocation, recover_parent};
use hh_subagent::result::{
    cancel_unresponsive, child_terminal_view, detach_to_child, drain_child_terminal,
    drain_on_parent_stop, record_child_result, ChildTerminalView,
};
use hh_subagent::spawn::{
    child_run_id_for, live_children, spawn, EnvDeriver, SpawnCtx, SpawnHook, SpawnPhase,
};
use hh_subagent::types::*;
use hh_wire::json::Json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

// ── fixtures ────────────────────────────────────────────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-subagent-test-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

/// A kernel-produced event minted against the run's current head (audit-grade
/// classes carry kernel provenance — `control.decision`, `lifecycle.*`).
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

/// A parent run + lease + a root `BudgetNode` with the dimensions the spawn
/// path consumes (`spawns`, `fan_out`, `delegation_depth`, `model_calls`).
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

fn parent_handle(run_id: &str) -> AuthorityHandle {
    AuthorityHandle {
        handle_id: HandleId("hnd-parent".to_string()),
        permission_ref: PinnedRef {
            semantic_id: "perm:root".to_string(),
            version_id: "v1".to_string(),
        },
        holder: Ref::pinned("agent_process", run_id.to_string()),
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
            expires_at: Some(HandleExpiry::Run(run_id.to_string())),
            revoked_by: None,
        },
        parent_handle: None,
        delegable: true,
        origin_basis: OriginBasis::Seal,
        basis_ref: "def:test#v1".to_string(),
        budget_ref: None,
        scope: DecisionScope::Session,
    }
}

fn live_coords(run_id: &str) -> LiveCoords {
    LiveCoords {
        effect_id: "eff-0".to_string(),
        turn_id: "turn-0".to_string(),
        run_id: run_id.to_string(),
        session: String::new(),
        seq: 0,
    }
}

/// The root budget spec — every gauge/feedback dim capped generously.
fn root_spec() -> BudgetSpec {
    BudgetSpec::hard_caps(
        BudgetMode::Pool,
        &[
            (DimensionKey::Primary(DimensionId::ModelCalls), 1_000),
            (DimensionKey::Primary(DimensionId::Spawns), 8),
            (DimensionKey::Primary(DimensionId::FanOut), 8),
            (DimensionKey::Primary(DimensionId::DelegationDepth), 4),
        ],
    )
}

/// Append a `control.decision{kind: delegate}` row — a real durable row (the
/// idempotency key's decision half + `causes[]`) — and update `p.decision`.
fn delegate_decision(p: &mut Parent, tag: i64) {
    let ev = kernel_ev(
        &p.store,
        &p.run_id,
        "control.decision",
        Json::obj([
            ("kind", Json::str("delegate")),
            ("delegation_reason", Json::str("parallelism")),
            ("decision_id", Json::str(format!("dec-{tag}"))),
        ]),
    );
    let id = ev.event_id.clone();
    p.store.append(&p.run_id, &p.lease, vec![ev]).unwrap();
    p.decision = EventRef {
        run_id: p.run_id.clone(),
        event_id: id,
    };
}

fn open_parent(tag: &str) -> Parent {
    let mut store = Store::open_test(dir(tag), 1_000).unwrap();
    // The spawned child inherits this audit-signer set — that is what makes
    // a final child checkpoint possible for the reciprocal `child_head`
    // anchor (DF-S2.5-2).
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
    delegate_decision(&mut p, 0);
    let mut account = Account::open(&mut p.store, &p.run_id).unwrap();
    p.budget_id = account
        .allocate(
            &p.lease,
            None,
            BudgetScope {
                kind: BudgetScopeKind::AgentProcess,
                target: p.run_id.clone(),
            },
            root_spec(),
        )
        .unwrap();
    let handle = parent_handle(&p.run_id);
    p.handle_id = handle.handle_id.clone();
    p.handles.handles.insert(p.handle_id.clone(), handle);
    // The parent owns its own workspace tree (the `holds` check's root).
    p.ownerships = OwnershipTable::project(
        &p.store,
        &[],
        vec![(p.run_id.clone(), OwnedObject::FsPathPrefix("/".to_string()))],
    )
    .unwrap();
    p
}

/// A minimal lawful `SubagentSpec` — native pinned child, delegated goal,
/// `fs_read` grant ⊆ the parent's, a small slice budget, `wait=background`.
fn spec() -> SubagentSpec {
    spec_with(BudgetMode::Slice, WaitMode::Background)
}

fn spec_with(mode: BudgetMode, wait: WaitMode) -> SubagentSpec {
    SubagentSpec {
        process: ChildProcess::Native {
            // The Ref's version_id is a pinned registry identity id (the
            // manifest's `harness_def_ref` — ADR-0137).
            harness_def: Ref::pinned("def:child", format!("sha256:{}", "d".repeat(64))),
            profile_binding: None,
            control_strategy: None,
            slots: None,
        },
        goal: GoalRecord {
            statement: Text::new(
                "subtask",
                "owner:parent",
                ProvenanceRecord::kernel("kernel:test", 0),
            ),
            success_criteria: vec![Ref::pinned("validator:noop", "v1")],
            unverifiable_reason: None,
            budget: Ref::pinned("budget:child", "v1"),
            origin: GoalOrigin::Delegated,
            parent: Some(Ref::pinned("goal:parent", "v1")),
        },
        role: ChildRole::Subagent,
        requested_grants: vec![grant(EffectDomain::FsRead, "/src", true)],
        ceiling: None,
        budget_spec: BudgetSpec::hard_caps(
            mode,
            &[
                (DimensionKey::Primary(DimensionId::ModelCalls), 10),
                (DimensionKey::Primary(DimensionId::Spawns), 0),
                (DimensionKey::Primary(DimensionId::FanOut), 0),
                (DimensionKey::Primary(DimensionId::DelegationDepth), 0),
            ],
        ),
        budget_mode: mode,
        environment: EnvIsolation::None,
        supplies: Supplies::default(),
        return_contract: ReturnContract::default(),
        wait_mode: wait,
        wait_timeout_ms: 60_000,
        on_parent_end: OnParentEnd::Teardown,
        delegation_reason: DelegationReason::Parallelism,
        topology_ref: None,
        stage_index: None,
        ownership_grants: Vec::new(),
        reserved_keys: Vec::new(),
        consistency_declarations: Vec::new(),
        merge_policy_ref: None,
        messaging_policy: None,
    }
}

/// The spawn seam set over `p` — disjoint field borrows keep `store` mutable
/// while the rest stay shared (the same split the control driver performs).
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
        coords: live_coords(run_id),
        reserve_ttl_ms: 30_000,
        hook: None,
    }
}

fn run(p: &mut Parent, s: &SubagentSpec) -> Result<Spawned, SpawnError> {
    let mut c = ctx(p);
    spawn(&mut c, s)
}

fn parent_class_count(p: &Parent, class: &str) -> usize {
    p.store
        .events(&p.run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == class)
        .count()
}

/// The parent's `child_terminal{child}` subscription exists.
fn has_child_terminal_sub(p: &Parent, child_run_id: &str) -> bool {
    p.store
        .wakeup_subscriptions(&p.run_id)
        .unwrap()
        .iter()
        .any(|w| {
            matches!(
                &w.trigger,
                hh_ledger::wakeup::Trigger::ChildTerminal { child_run_id: c } if c == child_run_id
            )
        })
}

// ── AC-R-2.6.3-1 — the typed spec + all spawn-side durable facts ────────────

#[test]
fn ac1_spawn_lands_every_required_fact() {
    let mut p = open_parent("ac1");
    let s = spec();
    let spawned = run(&mut p, &s).unwrap();
    assert!(spawned.child_run_id.starts_with("sub-"));
    assert_eq!(
        spawned.child_run_id,
        child_run_id_for(&p.decision, &s.spec_hash())
    );

    let events = p.store.events(&p.run_id).unwrap();
    let spawned_row = events
        .iter()
        .find(|e| e.class == "control.subagent.spawned")
        .expect("spawned row durable");
    let pl = &spawned_row.payload;
    for key in [
        "child_run_id",
        "delegation_ref",
        "child_handles",
        "parent_handle",
        "ceiling",
        "attenuation_delta",
        "budget_id",
        "mode",
        "isolation_mode",
        "supplies_digest",
        "return_contract_ref",
        "delegation_reason",
        "owner",
        "wait_mode",
        "on_parent_end",
        "depth",
        "parent_head_at_spawn",
        "ownership_grants",
        "reserved_keys",
        "consistency_declarations",
        "merge_policy_ref",
    ] {
        assert!(pl.get(key).is_some(), "spawned row missing {key}");
    }
    assert_eq!(
        pl.get("child_run_id").and_then(Json::as_str),
        Some(spawned.child_run_id.as_str())
    );
    assert_eq!(
        pl.get("delegation_reason").and_then(Json::as_str),
        Some("parallelism")
    );
    assert_eq!(pl.get("owner").and_then(Json::as_str), Some("code"));
    assert_eq!(pl.get("mode").and_then(Json::as_str), Some("slice"));
    // `causes[]` names the delegate decision (audit binding).
    assert!(spawned_row
        .causes
        .iter()
        .any(|c| c.event_id == p.decision.event_id));
    // One `security.permission.granted{origin_basis: delegation}` per minted handle.
    assert_eq!(
        events
            .iter()
            .filter(|e| e.class == "security.permission.granted"
                && e.payload.get("origin_basis").and_then(Json::as_str) == Some("delegation"))
            .count(),
        spawned.child_handles.len()
    );
    // The child run exists, carries parent_run_id + spawn_event.
    let manifest = p.store.manifest(&spawned.child_run_id).unwrap();
    assert_eq!(manifest.parent_run_id.as_deref(), Some(p.run_id.as_str()));
    // I-A6 reciprocal anchors: the child manifest's `parent_anchor{seq,
    // hash}` names the exact head the parent's spawned row recorded, and
    // the parent row carries the child scope id the checkpoint anchor
    // machinery reads.
    let parent_head = pl.get("parent_head_at_spawn").unwrap();
    let parent_anchor = manifest.extra.get("parent_anchor").unwrap();
    assert_eq!(
        parent_anchor.get("run_id").and_then(Json::as_str),
        Some(p.run_id.as_str())
    );
    assert_eq!(parent_anchor.get("seq"), parent_head.get("seq"));
    assert_eq!(parent_anchor.get("hash"), parent_head.get("hash"));
    assert_eq!(
        spawned_row.scope.child_run_id.as_deref(),
        Some(spawned.child_run_id.as_str())
    );
    let se = manifest
        .spawn_event
        .as_ref()
        .expect("spawn_event on child manifest");
    assert_eq!(se.run_id, p.run_id);
    assert_eq!(se.event_id, spawned.spawn_event.event_id);
    // The child's own writer lease was acquired.
    assert!(spawned.child_lease.is_some());
    // The parent's child_terminal subscription is durable.
    assert!(has_child_terminal_sub(&p, &spawned.child_run_id));
}

// ── AC-R-2.6.3-2 — idempotency on `(decision, H(spec))` ─────────────────────

#[test]
fn ac2_idempotent_replay_adopts_not_duplicates() {
    let mut p = open_parent("ac2");
    let s = spec();
    let first = run(&mut p, &s).unwrap();
    let second = run(&mut p, &s).unwrap();
    assert_eq!(first.child_run_id, second.child_run_id);
    assert_eq!(
        parent_class_count(&p, "control.subagent.spawned"),
        1,
        "no second spawned row"
    );
    // The first call's child writer lease is still held — adoption reports
    // `child_lease: None` (a same-holder WouldBlock is benign: the caller
    // already holds it).
    assert!(second.child_lease.is_none());
}

// ── AC-R-2.6.3-3 — attenuation: widening / non-delegable / ceiling ──────────

#[test]
fn ac3_widening_request_refuses_before_side_effects() {
    let mut p = open_parent("ac3");
    let mut s = spec();
    s.requested_grants = vec![grant(EffectDomain::NetEgress, "*", true)];
    let e = run(&mut p, &s).unwrap_err();
    assert!(matches!(
        e,
        SpawnError::Refused(SpawnRefused::AuthorityWidening { .. })
    ));
    // No spawn-side facts land (a `control.budget.reserved` may precede the
    // refusal — the reservation is a ledgered budget fact that dies with
    // the lease, never a delegated-to edge).
    assert_eq!(parent_class_count(&p, "control.subagent.spawned"), 0);
    assert!(!p.store.run_ids().iter().any(|r| r.starts_with("sub-")));
}

#[test]
fn ac3_non_delegable_parent_refuses() {
    let mut p = open_parent("ac3b");
    let h = p.handles.handles.get_mut(&p.handle_id).unwrap();
    h.delegable = false;
    for g in h.grants.iter_mut() {
        g.delegable = false;
    }
    let e = run(&mut p, &spec()).unwrap_err();
    assert!(matches!(
        e,
        SpawnError::Refused(SpawnRefused::NotDelegable { .. })
            | SpawnError::Refused(SpawnRefused::AuthorityWidening { .. })
    ));
}

#[test]
fn ac3_ceiling_attenuation_is_min() {
    let mut p = open_parent("ac3c");
    // Requesting above the parent's ceiling refuses (never clipped upward).
    let mut s = spec();
    s.ceiling = Some(AuthorityClass::Principal);
    let e = run(&mut p, &s).unwrap_err();
    assert!(matches!(
        e,
        SpawnError::Refused(SpawnRefused::AuthorityWidening { .. })
    ));
    // At/below parent: granted ceiling = min(requested, parent).
    let mut s2 = spec();
    s2.ceiling = Some(AuthorityClass::Delegate);
    let spawned = run(&mut p, &s2).unwrap();
    let events = p.store.events(&p.run_id).unwrap();
    let row = events
        .iter()
        .find(|e| e.class == "control.subagent.spawned")
        .unwrap();
    assert_eq!(
        row.payload.get("ceiling").and_then(Json::as_str),
        Some("delegate")
    );
    assert!(!spawned.child_handles.is_empty());
}

// ── AC-R-2.6.3-5 — depth + fan_out gauges refuse typed ──────────────────────

#[test]
fn ac5_depth_gauge_refuses_at_cap() {
    let mut p = open_parent("ac5a");
    let s = spec();
    let mut c = ctx(&mut p);
    c.parent_depth = 4; // cap is 4 — depth+1 = 5 > 4
    let e = spawn(&mut c, &s).unwrap_err();
    assert!(matches!(e, SpawnError::Refused(SpawnRefused::Depth { .. })));
}

#[test]
fn ac5_fan_out_gauge_refuses_at_cap() {
    let mut p = open_parent("ac5b");
    // Eight children (the cap) spawn cleanly — each under a distinct decision
    // (the idempotency key is (decision, H(spec)) — same key = same spawn).
    for i in 1..=8 {
        delegate_decision(&mut p, i);
        run(&mut p, &spec()).unwrap_or_else(|e| panic!("spawn {i} refused: {e:?}"));
    }
    assert_eq!(live_children(&p.store, &p.run_id).unwrap(), 8);
    // The ninth refuses fan_out.
    delegate_decision(&mut p, 9);
    let e = run(&mut p, &spec()).unwrap_err();
    assert!(matches!(
        e,
        SpawnError::Refused(SpawnRefused::FanOut { .. })
    ));
}

// ── AC-R-2.6.3-6 — results are artifacts/events, never transcripts ──────────

#[test]
fn ac6_result_row_is_refs_never_transcript() {
    let mut p = open_parent("ac6");
    let s = spec();
    let spawned = run(&mut p, &s).unwrap();
    let child_lease = spawned.child_lease.clone().unwrap();
    // The child finishes — a terminal row on its own ledger.
    let fin = kernel_ev(
        &p.store,
        &spawned.child_run_id,
        "lifecycle.run.finished",
        Json::obj([("stop_reason", Json::str("goal_achieved"))]),
    );
    p.store
        .append(&spawned.child_run_id, &child_lease, vec![fin])
        .unwrap();
    // The signed final checkpoint names the child's immutable terminal head;
    // the parent's `child_head` cites the checkpoint-covered tip, not the
    // checkpoint event itself.
    let checkpoint = p
        .store
        .checkpoint(
            &spawned.child_run_id,
            &child_lease,
            CheckpointKind::Final,
            &mut FixedSigner::new("test-key", b"s4.6-test-signing-key".to_vec()),
        )
        .unwrap();
    let claim = hh_ledger::tree::parse_checkpoint(&checkpoint.payload).unwrap();
    let mut view = child_terminal_view(&p.store, &spawned.child_run_id).unwrap();
    assert!(view.finished);
    let child_head = view.child_head.as_ref().unwrap();
    assert_eq!(
        child_head.get("seq").and_then(Json::as_int),
        Some(claim.tree_size.unwrap() as i64 - 1)
    );
    assert_eq!(
        child_head.get("hash").and_then(Json::as_str),
        claim.chain_hash.as_deref()
    );
    assert_eq!(
        child_head.get("checkpoint_ref").and_then(Json::as_str),
        Some(hh_ledger::tree::checkpoint_idp(&claim.payload).as_str())
    );
    view.artifacts = vec![Json::str("art:child-out")];
    view.summary = Some(Json::str("art:child-summary"));
    let pair = record_child_result(
        &mut p.store,
        &p.run_id,
        &p.lease,
        &spawned.child_run_id,
        &s,
        &view,
        &spawned.budget_id,
    )
    .unwrap();
    assert_eq!(pair.result.outcome_class, "result");
    assert!(
        pair.result.budget_released,
        "slice remainder returns at terminal"
    );
    // The durable row carries refs — never a transcript/model_io member.
    let row = p
        .store
        .events(&p.run_id)
        .unwrap()
        .iter()
        .find(|e| e.class == "control.subagent.result")
        .expect("result row durable")
        .clone();
    assert!(row.payload.get("transcript").is_none());
    assert!(row.payload.get("model_io").is_none());
    assert_eq!(
        row.scope.child_run_id.as_deref(),
        Some(spawned.child_run_id.as_str())
    );
    assert_eq!(row.payload.get("child_head"), view.child_head.as_ref());
    // The row's envelope provenance is kernel (Rule P — the row is a
    // kernel fact); the result's re-entry label rides in `payload.reentry`
    // at `delegate` with `derived_from.kind = subagent_result` (ADR-0053 D-3).
    assert_eq!(
        row.payload
            .get("reentry")
            .and_then(|r| r.get("authority"))
            .and_then(Json::as_str),
        Some("delegate")
    );
    assert_eq!(
        row.payload
            .get("reentry")
            .and_then(|r| r.get("derived_from"))
            .and_then(|d| d.get("kind"))
            .and_then(Json::as_str),
        Some("subagent_result")
    );
    // The parent checkpoint records the spawned edge as a signed cross-run
    // anchor (`scope.child_run_id` is the durable join key; ADR-0067 §4).
    let parent_checkpoint = p
        .store
        .checkpoint(
            &p.run_id,
            &p.lease,
            CheckpointKind::OnDemand,
            &mut FixedSigner::new("test-key", b"s4.6-test-signing-key".to_vec()),
        )
        .unwrap();
    let parent_claim = hh_ledger::tree::parse_checkpoint(&parent_checkpoint.payload).unwrap();
    let anchor = parent_claim
        .cross_run_anchors
        .iter()
        .find(|a| a.get("other_run").and_then(Json::as_str) == Some(spawned.child_run_id.as_str()))
        .expect("spawned child anchored in parent checkpoint");
    assert_eq!(
        anchor.get("relation").and_then(Json::as_str),
        Some("subagent")
    );
    let child_leaves: Vec<String> = p
        .store
        .events(&spawned.child_run_id)
        .unwrap()
        .iter()
        .map(|e| e.hash.clone())
        .collect();
    let other_head = anchor.get("other_head").unwrap();
    assert_eq!(
        other_head.get("tree_size").and_then(Json::as_int),
        Some(child_leaves.len() as i64)
    );
    assert_eq!(
        other_head.get("tree_head").and_then(Json::as_str),
        Some(hh_ledger::tree::mth(&child_leaves).as_str())
    );
    // The live-children fold sees a terminal row — the child no longer counts.
    assert_eq!(live_children(&p.store, &p.run_id).unwrap(), 0);
}

// ── AC-R-2.6.3-7 — cancellation + drain (C-1) ────────────────────────────────

#[test]
fn ac7_drain_records_terminal_for_finished_children() {
    let mut p = open_parent("ac7");
    let s = spec();
    let spawned = run(&mut p, &s).unwrap();
    let child_lease = spawned.child_lease.clone().unwrap();
    let fin = kernel_ev(
        &p.store,
        &spawned.child_run_id,
        "lifecycle.run.finished",
        Json::obj([("stop_reason", Json::str("cancelled"))]),
    );
    p.store
        .append(&spawned.child_run_id, &child_lease, vec![fin])
        .unwrap();
    let specs = BTreeMap::from([(spawned.child_run_id.clone(), s.clone())]);
    let done = drain_child_terminal(&mut p.store, &p.run_id, &p.lease, &specs).unwrap();
    assert_eq!(done, vec![spawned.child_run_id.clone()]);
    assert_eq!(parent_class_count(&p, "control.subagent.result"), 1);
    // A second drain is a no-op (the terminal row exists — idempotent).
    let again = drain_child_terminal(&mut p.store, &p.run_id, &p.lease, &specs).unwrap();
    assert!(again.is_empty());
}

// ── AC-R-2.6.3-8 — detach_to_child (T6 arm) ─────────────────────────────────

#[test]
fn ac8_detach_lands_detached_row() {
    let mut p = open_parent("ac8");
    let mut s = spec();
    s.on_parent_end = OnParentEnd::DetachToChild;
    let spawned = run(&mut p, &s).unwrap();
    detach_to_child(
        &mut p.store,
        &p.run_id,
        &p.lease,
        &spawned.child_run_id,
        &p.decision,
    )
    .unwrap();
    let row = p
        .store
        .events(&p.run_id)
        .unwrap()
        .iter()
        .find(|e| e.class == "control.subagent.detached")
        .expect("detached row durable")
        .clone();
    assert_eq!(
        row.payload.get("child_run_id").and_then(Json::as_str),
        Some(spawned.child_run_id.as_str())
    );
    assert_eq!(
        row.scope.child_run_id.as_deref(),
        Some(spawned.child_run_id.as_str())
    );
}

// ── AC-R-2.6.3-9 — ownership: grant ⊆ check + transfer rows ─────────────────

#[test]
fn ac9_ownership_grant_and_transfer_rows() {
    let mut p = open_parent("ac9");
    let mut s = spec();
    s.ownership_grants = vec![OwnedObject::FsPathPrefix("/src".to_string())];
    let spawned = run(&mut p, &s).unwrap();
    let rows: Vec<_> = p
        .store
        .events(&p.run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == "control.ownership.transferred")
        .cloned()
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].payload.get("to").and_then(Json::as_str),
        Some(spawned.child_run_id.as_str())
    );
    // The projection sees the child as the owner.
    let table = OwnershipTable::project(
        &p.store,
        &[p.run_id.clone()],
        vec![(p.run_id.clone(), OwnedObject::FsPathPrefix("/".to_string()))],
    )
    .unwrap();
    assert_eq!(
        table.owner_of(&OwnedObject::FsPathPrefix("/src/lib".to_string())),
        Some(spawned.child_run_id.as_str())
    );
}

#[test]
fn ac9_ownership_widening_refuses_not_owned() {
    let mut p = open_parent("ac9b");
    // A parent holding only `/owned` cannot grant `/other`.
    p.ownerships = OwnershipTable::project(
        &p.store,
        &[],
        vec![(
            p.run_id.clone(),
            OwnedObject::FsPathPrefix("/owned".to_string()),
        )],
    )
    .unwrap();
    let mut s = spec();
    s.ownership_grants = vec![OwnedObject::FsPathPrefix("/other".to_string())];
    let e = run(&mut p, &s).unwrap_err();
    assert!(matches!(
        e,
        SpawnError::Refused(SpawnRefused::NotOwned { .. })
    ));
}

// ── AC-R-2.6.3-10 — parent-mediated messaging ───────────────────────────────

#[test]
fn ac10_peer_message_sent_and_refused_rows() {
    let mut p = open_parent("ac10");
    let s = spec();
    let spawned = run(&mut p, &s).unwrap();
    let parent_run = p.run_id.clone();
    let child_run = spawned.child_run_id.clone();
    let policy = MessagingPolicy::default();
    // Parent → child send (direction `child` — the policy admits by default).
    let first_message_id;
    {
        let mut c = SendCtx {
            store: &mut p.store,
            sender_run_id: &parent_run,
            sender_lease: &p.lease,
            to: &child_run,
            body: b"hello",
            delivery_mode: hh_ledger::wakeup::DeliveryMode::FollowUp,
            idempotency_key: "ac10-msg-1",
            policy: &policy,
            caps: MessageCaps::KERNEL,
            direction: "child",
            ctx_label: &Json::str("parent"),
            caused_by: Some(&p.decision),
        };
        let receipt = send_message(&mut c).unwrap();
        assert_eq!(receipt.status, MessageStatus::Queued);
        first_message_id = receipt.message_id;
    }
    let sent = p.store.events(&parent_run).unwrap();
    let sent_row = sent
        .iter()
        .find(|e| e.event_id == first_message_id)
        .expect("sent row");
    let body_ref = sent_row
        .payload
        .get("body_ref")
        .and_then(Json::as_str)
        .expect("body ref")
        .to_string();
    let parsed = hh_identity::idp::parse_id(&body_ref).unwrap();
    let body = p
        .store
        .get_blob(&hh_identity::idp::ContentAddress {
            idp: "idp/1",
            algorithm: "sha256",
            digest: parsed.digest_hex,
            media_type: "text/plain".to_string(),
            size: 0,
        })
        .expect("message body persists off-row");
    assert_eq!(body, b"hello");
    assert_eq!(
        sent_row
            .payload
            .get("idempotency_key")
            .and_then(Json::as_str),
        Some("ac10-msg-1")
    );
    // Same key + same arguments returns the durable send, never a second
    // row. Same key + different arguments is a caller conflict.
    {
        let mut retry = SendCtx {
            store: &mut p.store,
            sender_run_id: &parent_run,
            sender_lease: &p.lease,
            to: &child_run,
            body: b"hello",
            delivery_mode: hh_ledger::wakeup::DeliveryMode::FollowUp,
            idempotency_key: "ac10-msg-1",
            policy: &policy,
            caps: MessageCaps::KERNEL,
            direction: "child",
            ctx_label: &Json::str("parent"),
            caused_by: Some(&p.decision),
        };
        let receipt = send_message(&mut retry).unwrap();
        assert_eq!(receipt.message_id, first_message_id);
        assert_eq!(
            p.store
                .events(&parent_run)
                .unwrap()
                .iter()
                .filter(|e| e.class == "control.message.sent")
                .count(),
            1
        );
        let mut conflict = SendCtx {
            store: &mut p.store,
            sender_run_id: &parent_run,
            sender_lease: &p.lease,
            to: &child_run,
            body: b"different",
            delivery_mode: hh_ledger::wakeup::DeliveryMode::FollowUp,
            idempotency_key: "ac10-msg-1",
            policy: &policy,
            caps: MessageCaps::KERNEL,
            direction: "child",
            ctx_label: &Json::str("parent"),
            caused_by: Some(&p.decision),
        };
        assert!(matches!(
            send_message(&mut conflict),
            Err(SpawnError::Kernel(_))
        ));
    }
    // Sibling direction is refused by the OQ-428 default — a typed,
    // ledgered `control.message.refused{reason: policy}` on the *sender's*
    // ledger (the child's).
    {
        let child_lease = spawned.child_lease.clone().unwrap();
        let mut c2 = SendCtx {
            store: &mut p.store,
            sender_run_id: &child_run,
            sender_lease: &child_lease,
            to: "sub-other",
            body: b"nope",
            delivery_mode: hh_ledger::wakeup::DeliveryMode::FollowUp,
            idempotency_key: "ac10-msg-2",
            policy: &policy,
            caps: MessageCaps::KERNEL,
            direction: "sibling",
            ctx_label: &Json::str("child"),
            caused_by: None,
        };
        let refused = send_message(&mut c2).unwrap();
        assert!(matches!(
            refused.status,
            MessageStatus::Refused(MessageRefused::Policy { .. })
        ));
    }
    let child_events = p.store.events(&child_run).unwrap();
    assert!(child_events
        .iter()
        .any(|e| e.class == "control.message.refused"
            && e.payload.get("reason").and_then(Json::as_str) == Some("policy")));
    // Parent-mediated delivery: the receiver-side occurrence lands under the
    // *receiver's* lease (lazy peer_message subscription + dedupe).
    let tree = relay_set(&p.store, &parent_run).unwrap();
    let child_lease = spawned.child_lease.clone().unwrap();
    let delivered = inbox_scan(&mut p.store, &child_run, &child_lease, &tree).unwrap();
    assert_eq!(delivered.len(), 1);
    // A second scan dedupes by occurrence key — no second delivery.
    let again = inbox_scan(&mut p.store, &child_run, &child_lease, &tree).unwrap();
    assert!(again.is_empty());
}

// ── AC-R-2.6.3-11 — merge (single_writer / parent_decides + veto) ────────────

#[test]
fn ac11_single_writer_merge_completes_clean_inputs() {
    let mut p = open_parent("ac11");
    let spawned = run(&mut p, &spec()).unwrap();
    // The child owns /src/child — its write there merges.
    let parents: BTreeMap<String, Option<String>> = BTreeMap::new();
    let baseline: BTreeMap<String, String> = BTreeMap::new();
    let child_owns: BTreeMap<String, Vec<OwnedObject>> = BTreeMap::from([(
        spawned.child_run_id.clone(),
        vec![OwnedObject::FsPathPrefix("/src/child".to_string())],
    )]);
    let inputs = vec![MergeInput {
        child_run_id: spawned.child_run_id.clone(),
        changes: BTreeMap::from([(
            "/src/child/out.txt".to_string(),
            (None, Some("ref:new".to_string())),
        )]),
    }];
    let out = {
        let mut c = MergeCtx {
            store: &mut p.store,
            parent_run_id: &p.run_id,
            parent_lease: &p.lease,
            decision: &p.decision,
            ownerships: &p.ownerships,
            policy: MergePolicy::SingleWriter,
            inputs,
            parent_versions: &parents,
            baseline: &baseline,
            child_ownerships: &child_owns,
        };
        merge(&mut c).unwrap()
    };
    assert!(out.veto.is_none(), "clean merge: {out:?}");
    let events = p.store.events(&p.run_id).unwrap();
    assert!(events.iter().any(|e| e.class == "control.merge.started"));
    assert!(events.iter().any(|e| e.class == "control.merge.completed"));
    assert!(out.report.is_some());
}

#[test]
fn ac11_parent_decides_conflict_records_resolved() {
    let mut p = open_parent("ac11b");
    let spawned = run(&mut p, &spec()).unwrap();
    // Both parent and child wrote /shared — under parent_decides the conflict
    // is a recorded `control.merge.resolved` (the parent's side wins pending
    // the declared call; escalate is the H7 arm and refuses typed).
    let parents: BTreeMap<String, Option<String>> =
        BTreeMap::from([("/shared".to_string(), Some("ref:parent".to_string()))]);
    let baseline: BTreeMap<String, String> =
        BTreeMap::from([("/shared".to_string(), "ref:base".to_string())]);
    // The child was granted `/shared` at spawn — its write there is a
    // recorded conflict the parent decides, not a NotOwner veto.
    let child_owns: BTreeMap<String, Vec<OwnedObject>> = BTreeMap::from([(
        spawned.child_run_id.clone(),
        vec![OwnedObject::FsPathPrefix("/shared".to_string())],
    )]);
    let inputs = vec![MergeInput {
        child_run_id: spawned.child_run_id.clone(),
        changes: BTreeMap::from([(
            "/shared".to_string(),
            (Some("ref:base".to_string()), Some("ref:child".to_string())),
        )]),
    }];
    let out = {
        let mut c = MergeCtx {
            store: &mut p.store,
            parent_run_id: &p.run_id,
            parent_lease: &p.lease,
            decision: &p.decision,
            ownerships: &p.ownerships,
            policy: MergePolicy::ParentDecides,
            inputs,
            parent_versions: &parents,
            baseline: &baseline,
            child_ownerships: &child_owns,
        };
        merge(&mut c).unwrap()
    };
    assert!(out.veto.is_none());
    let events = p.store.events(&p.run_id).unwrap();
    assert!(events
        .iter()
        .any(|e| e.class == "control.merge.resolved" || e.class == "control.merge.completed"));
}

#[test]
fn ac11_single_writer_conflict_vetoes() {
    let mut p = open_parent("ac11c");
    let spawned = run(&mut p, &spec()).unwrap();
    let parents: BTreeMap<String, Option<String>> =
        BTreeMap::from([("/shared".to_string(), Some("ref:parent".to_string()))]);
    let baseline: BTreeMap<String, String> =
        BTreeMap::from([("/shared".to_string(), "ref:base".to_string())]);
    let child_owns: BTreeMap<String, Vec<OwnedObject>> = BTreeMap::new();
    let inputs = vec![MergeInput {
        child_run_id: spawned.child_run_id.clone(),
        changes: BTreeMap::from([(
            "/shared".to_string(),
            (Some("ref:base".to_string()), Some("ref:child".to_string())),
        )]),
    }];
    let out = {
        let mut c = MergeCtx {
            store: &mut p.store,
            parent_run_id: &p.run_id,
            parent_lease: &p.lease,
            decision: &p.decision,
            ownerships: &p.ownerships,
            policy: MergePolicy::SingleWriter,
            inputs,
            parent_versions: &parents,
            baseline: &baseline,
            child_ownerships: &child_owns,
        };
        merge(&mut c).unwrap()
    };
    assert!(matches!(
        out.veto,
        Some(MergeVeto::Conflict { .. }) | Some(MergeVeto::NotOwner { .. })
    ));
}

// ── AC-R-2.6.3-12 — KP battery: kill at every durable boundary ──────────────

struct KillAt(SpawnPhase);
impl SpawnHook for KillAt {
    fn at(&mut self, phase: SpawnPhase) -> Result<(), SpawnError> {
        if phase == self.0 {
            Err(SpawnError::Kernel(format!("kill at {phase:?}")))
        } else {
            Ok(())
        }
    }
}

fn spawn_kill_at(p: &mut Parent, s: &SubagentSpec, phase: SpawnPhase) -> SpawnError {
    let mut c = ctx(p);
    let mut hook = KillAt(phase);
    c.hook = Some(&mut hook);
    spawn(&mut c, s).unwrap_err()
}

#[test]
fn ac12_kill_before_spawned_leaves_no_child_facts() {
    for phase in [
        SpawnPhase::AfterEnvelopeChecks,
        SpawnPhase::AfterReserve,
        SpawnPhase::AfterDelegate,
        SpawnPhase::AfterAllocate,
        SpawnPhase::AfterOwnership,
    ] {
        let mut p = open_parent(&format!("kp-{phase:?}"));
        let e = spawn_kill_at(&mut p, &spec(), phase);
        assert!(matches!(e, SpawnError::Kernel(_)), "{phase:?}: {e:?}");
        assert_eq!(
            parent_class_count(&p, "control.subagent.spawned"),
            0,
            "{phase:?}: no spawned row durable"
        );
        assert!(
            !p.store.run_ids().iter().any(|r| r.starts_with("sub-")),
            "{phase:?}: no child run"
        );
    }
}

#[test]
fn ac12_kill_after_spawned_recovers_by_replay() {
    // KP-21's late half: the spawned row is durable, the child run is not —
    // a retried `spawn` (same decision + spec) finishes the interrupted
    // spawn rather than orphaning it (deterministic child_run_id).
    let mut p = open_parent("kp21");
    let s = spec();
    let _ = spawn_kill_at(&mut p, &s, SpawnPhase::AfterSpawnedAppend);
    assert_eq!(parent_class_count(&p, "control.subagent.spawned"), 1);
    let child_id = child_run_id_for(&p.decision, &s.spec_hash());
    assert!(!p.store.has_run(&child_id), "child run not yet created");
    let recovered = run(&mut p, &s).unwrap();
    assert_eq!(recovered.child_run_id, child_id);
    assert!(p.store.has_run(&child_id));
    assert_eq!(
        parent_class_count(&p, "control.subagent.spawned"),
        1,
        "still one row"
    );
    assert!(has_child_terminal_sub(&p, &child_id));
    let account = Account::open(&mut p.store, &p.run_id).unwrap();
    assert!(
        account
            .tree
            .reservations
            .values()
            .all(|r| !r.outstanding || r.quantity.get(DimensionId::Spawns) == 0),
        "the adopted spawn freed its own spawns reservation"
    );
}

#[test]
fn ac12_kill_after_child_open_recovers_subscription() {
    // KP-16's seam: the child run exists, the child_terminal subscription
    // does not — the replay path converges to a subscribed child.
    let mut p = open_parent("kp16");
    let s = spec();
    let _ = spawn_kill_at(&mut p, &s, SpawnPhase::AfterChildOpen);
    let child_id = child_run_id_for(&p.decision, &s.spec_hash());
    assert!(p.store.has_run(&child_id));
    assert!(!has_child_terminal_sub(&p, &child_id));
    let recovered = run(&mut p, &s).unwrap();
    assert_eq!(recovered.child_run_id, child_id);
    assert!(has_child_terminal_sub(&p, &child_id));
    let account = Account::open(&mut p.store, &p.run_id).unwrap();
    assert!(
        account
            .tree
            .reservations
            .values()
            .all(|r| !r.outstanding || r.quantity.get(DimensionId::Spawns) == 0),
        "the adopted spawn freed its own spawns reservation"
    );
}

#[test]
fn ac12_recovery_report_scans_and_repairs() {
    let mut p = open_parent("recovery");
    let s = spec();
    // Kill between spawned-row and child-open — recover_parent sees the
    // interrupted spawn and cancels the orphaned edge (typed, retained).
    let _ = spawn_kill_at(&mut p, &s, SpawnPhase::AfterSpawnedAppend);
    let report = recover_parent(&mut p.store, &p.run_id, &p.lease).unwrap();
    assert!(
        !report.interrupted.is_empty()
            || !report.resubscribed.is_empty()
            || !report.completed.is_empty(),
        "recovery saw the interrupted spawn: {report:?}"
    );
    // Idempotent — a second pass repairs nothing.
    let again = recover_parent(&mut p.store, &p.run_id, &p.lease).unwrap();
    assert!(again.interrupted.is_empty() && again.resubscribed.is_empty());
    let account = Account::open(&mut p.store, &p.run_id).unwrap();
    assert!(
        account
            .tree
            .reservations
            .values()
            .all(|r| !r.outstanding || r.quantity.get(DimensionId::Spawns) == 0),
        "the cancelled interrupted spawn released its reservation"
    );
}

// ── AC-R-2.1.6-4 — slice/pool conservation through spawn ────────────────────

#[test]
fn ac2164_slice_child_over_parent_remaining_refuses() {
    let mut p = open_parent("ac2164");
    let mut s = spec();
    // A slice asking for more than the parent's remaining refuses typed.
    s.budget_spec = BudgetSpec::hard_caps(
        BudgetMode::Slice,
        &[(DimensionKey::Primary(DimensionId::ModelCalls), 100_000)],
    );
    s.budget_mode = BudgetMode::Slice;
    let e = run(&mut p, &s).unwrap_err();
    assert!(
        matches!(
            e,
            SpawnError::Refused(SpawnRefused::BudgetExceedsParent { .. })
        ),
        "over-remaining slice refused typed, got: {e:?}"
    );
}

#[test]
fn ac2164_pool_mode_child_allocates() {
    let mut p = open_parent("ac2164b");
    let s = spec_with(BudgetMode::Pool, WaitMode::Background);
    let spawned = run(&mut p, &s).unwrap();
    let events = p.store.events(&p.run_id).unwrap();
    let row = events
        .iter()
        .find(|e| e.class == "control.subagent.spawned")
        .unwrap();
    assert_eq!(row.payload.get("mode").and_then(Json::as_str), Some("pool"));
    assert!(!spawned.budget_id.is_empty());
}

// ── environment derivation (step 5 arms) ────────────────────────────────────

struct TestDeriver;
impl EnvDeriver for TestDeriver {
    fn derive(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        parent_env_id: &str,
        _mode: DeriveMode,
        _scope: Option<&str>,
        _on_parent_end: OnParentEnd,
        derived_for: &str,
    ) -> Result<String, String> {
        // The double lands an equivalent durable marker so the phase
        // boundary stays honest (the real driver writes `environment.derived`).
        let ev = kernel_ev(
            store,
            &lease.run_id,
            "action.environment.derived",
            Json::obj([
                ("parent_env", Json::str(parent_env_id)),
                ("child_env", Json::str("env-child-1")),
                ("derived_for", Json::str(derived_for)),
            ]),
        );
        store
            .append(&lease.run_id, lease, vec![ev])
            .map_err(|e| e.to_string())?;
        Ok("env-child-1".to_string())
    }
}

#[test]
fn env_derive_lands_env_handle_on_spawned_row() {
    let mut p = open_parent("env");
    let mut s = spec();
    s.environment = EnvIsolation::Derive {
        mode: DeriveMode::FreshFromImage,
        spec: Json::obj([("scope", Json::str("/work"))]),
    };
    let mut drv = TestDeriver;
    let spawned = {
        let mut c = ctx(&mut p);
        c.parent_env_id = Some("env-parent");
        c.env_driver = Some(&mut drv);
        spawn(&mut c, &s).unwrap()
    };
    assert_eq!(spawned.env_handle_id.as_deref(), Some("env-child-1"));
    let events = p.store.events(&p.run_id).unwrap();
    assert!(events
        .iter()
        .any(|e| e.class == "action.environment.derived"));
    let row = events
        .iter()
        .find(|e| e.class == "control.subagent.spawned")
        .unwrap();
    assert_eq!(
        row.payload.get("env_handle_id").and_then(Json::as_str),
        Some("env-child-1")
    );
}

#[test]
fn env_derive_without_driver_refuses_mode_unsupported() {
    let mut p = open_parent("env2");
    let mut s = spec();
    s.environment = EnvIsolation::Derive {
        mode: DeriveMode::FreshFromImage,
        spec: Json::obj([("scope", Json::str("/work"))]),
    };
    let e = run(&mut p, &s).unwrap_err();
    assert!(matches!(
        e,
        SpawnError::Refused(SpawnRefused::ModeUnsupported { .. })
    ));
}

#[test]
fn env_share_refuses_stage5_arm() {
    let mut p = open_parent("env3");
    let mut s = spec();
    s.environment = EnvIsolation::Derive {
        mode: DeriveMode::Share,
        spec: Json::obj([]),
    };
    let mut drv = TestDeriver;
    let e = {
        let mut c = ctx(&mut p);
        c.parent_env_id = Some("env-parent");
        c.env_driver = Some(&mut drv);
        spawn(&mut c, &s).unwrap_err()
    };
    assert!(matches!(
        e,
        SpawnError::Refused(SpawnRefused::ModeUnsupported { .. })
    ));
}

// ── spec-shape refusals (step 2 pre-checks) ─────────────────────────────────

#[test]
fn unpinned_harness_def_refuses_definition_unresolvable() {
    let mut p = open_parent("defunres");
    let mut s = spec();
    s.process = ChildProcess::Native {
        harness_def: Ref::selected("def:child", "v2"),
        profile_binding: None,
        control_strategy: None,
        slots: None,
    };
    let e = run(&mut p, &s).unwrap_err();
    assert!(matches!(
        e,
        SpawnError::Refused(SpawnRefused::DefinitionUnresolvable { .. })
    ));
}

#[test]
fn non_delegated_goal_origin_refuses() {
    let mut p = open_parent("goal");
    let mut s = spec();
    s.goal.origin = GoalOrigin::Human;
    let e = run(&mut p, &s).unwrap_err();
    assert!(matches!(
        e,
        SpawnError::Refused(SpawnRefused::DefinitionUnresolvable { .. })
    ));
}

#[test]
fn tools_not_in_parent_table_refuse() {
    let mut p = open_parent("tools");
    let mut s = spec();
    s.supplies.tools = vec![Ref::pinned("tool:nope", "v1")];
    let e = run(&mut p, &s).unwrap_err();
    assert!(matches!(
        e,
        SpawnError::Refused(SpawnRefused::DefinitionUnresolvable { .. })
    ));
}

#[test]
fn supplies_tools_subset_of_sealed_table_admits() {
    let mut p = open_parent("tools2");
    p.tools = vec![Ref::pinned("tool:fs", "v1"), Ref::pinned("tool:sh", "v1")];
    let mut s = spec();
    s.supplies.tools = vec![Ref::pinned("tool:fs", "v1")];
    let spawned = run(&mut p, &s).unwrap();
    assert!(spawned.child_lease.is_some());
}

#[test]
fn ac12_pre_spawn_retry_reuses_durable_effects() {
    // KP-21's early half: a kill before `control.subagent.spawned` may leave
    // the reservation / allocation / ownership rows durable. The retry is
    // keyed by the deterministic child id and must adopt them, never mint a
    // second effect.
    for phase in [
        SpawnPhase::AfterReserve,
        SpawnPhase::AfterAllocate,
        SpawnPhase::AfterOwnership,
    ] {
        let mut p = open_parent(&format!("pre-{phase:?}"));
        let mut s = spec();
        s.ownership_grants = vec![OwnedObject::FsPathPrefix("/src".to_string())];
        let child_id = child_run_id_for(&p.decision, &s.spec_hash());
        let _ = spawn_kill_at(&mut p, &s, phase);
        let reserved_before = parent_class_count(&p, "control.budget.reserved");
        let allocated_before = p
            .store
            .events(&p.run_id)
            .unwrap()
            .iter()
            .filter(|e| {
                e.class == "control.budget.allocated"
                    && e.payload
                        .get("scope")
                        .and_then(|sc| sc.get("target"))
                        .and_then(Json::as_str)
                        == Some(child_id.as_str())
            })
            .count();
        let transfers_before = parent_class_count(&p, "control.ownership.transferred");
        let recovered = run(&mut p, &s).unwrap();
        assert_eq!(recovered.child_run_id, child_id);
        assert_eq!(
            parent_class_count(&p, "control.budget.reserved"),
            reserved_before,
            "{phase:?}: retry re-used the outstanding spawns reservation"
        );
        assert_eq!(
            p.store
                .events(&p.run_id)
                .unwrap()
                .iter()
                .filter(|e| {
                    e.class == "control.budget.allocated"
                        && e.payload
                            .get("scope")
                            .and_then(|sc| sc.get("target"))
                            .and_then(Json::as_str)
                            == Some(child_id.as_str())
                })
                .count(),
            allocated_before.max(1),
            "{phase:?}: one child budget node"
        );
        assert_eq!(
            parent_class_count(&p, "control.ownership.transferred"),
            transfers_before.max(1),
            "{phase:?}: one ownership transfer"
        );
        assert_eq!(parent_class_count(&p, "control.subagent.spawned"), 1);
    }
}

#[test]
fn ac12_derive_retry_reuses_durable_env_handle() {
    let mut p = open_parent("pre-derive");
    let mut s = spec();
    s.environment = EnvIsolation::Derive {
        mode: DeriveMode::FreshFromImage,
        spec: Json::obj([("scope", Json::str("/work"))]),
    };
    let mut deriver = TestDeriver;
    {
        let mut c = ctx(&mut p);
        c.parent_env_id = Some("env-parent");
        c.env_driver = Some(&mut deriver);
        let mut hook = KillAt(SpawnPhase::AfterDerive);
        c.hook = Some(&mut hook);
        let _ = spawn(&mut c, &s).unwrap_err();
    }
    assert_eq!(parent_class_count(&p, "action.environment.derived"), 1);
    {
        let mut c = ctx(&mut p);
        c.parent_env_id = Some("env-parent");
        c.env_driver = Some(&mut deriver);
        let recovered = spawn(&mut c, &s).unwrap();
        assert_eq!(recovered.env_handle_id.as_deref(), Some("env-child-1"));
    }
    assert_eq!(
        parent_class_count(&p, "action.environment.derived"),
        1,
        "the retry adopted the derivation keyed by the deterministic child id"
    );
    assert_eq!(parent_class_count(&p, "control.subagent.spawned"), 1);
}

#[test]
fn merge_report_projects_from_durable_blob() {
    let mut p = open_parent("merge-projection");
    let s = spec();
    let spawned = run(&mut p, &s).unwrap();
    let child = spawned.child_run_id.clone();
    let mut child_ownerships = BTreeMap::new();
    child_ownerships.insert(
        child.clone(),
        vec![OwnedObject::FsPathPrefix("/src".to_string())],
    );
    let mut changes = BTreeMap::new();
    changes.insert("/src/a".to_string(), (None, Some("sha256:new".to_string())));
    let parent_versions = BTreeMap::new();
    let baseline = BTreeMap::new();
    let mut c = MergeCtx {
        store: &mut p.store,
        parent_run_id: &p.run_id,
        parent_lease: &p.lease,
        decision: &p.decision,
        ownerships: &p.ownerships,
        policy: MergePolicy::SingleWriter,
        inputs: vec![MergeInput {
            child_run_id: child,
            changes,
        }],
        parent_versions: &parent_versions,
        baseline: &baseline,
        child_ownerships: &child_ownerships,
    };
    let out = merge(&mut c).unwrap();
    let report = out.report.expect("clean merge reports");
    let projected = project_merge_report(&p.store, &p.run_id, &out.merge_id)
        .unwrap()
        .expect("merge_report_ref resolves");
    assert_eq!(projected, report);
}

#[test]
fn return_contract_violations_are_typed_on_result() {
    let mut p = open_parent("contract-violation");
    let mut s = spec();
    s.return_contract = ReturnContract {
        artifacts: vec![ArtifactDecl {
            kind: "required_output".into(),
            schema: None,
            required: true,
        }],
        summary: None,
        claims: true,
    };
    let spawned = run(&mut p, &s).unwrap();
    let view = ChildTerminalView {
        finished: true,
        stop_reason: "goal_achieved".into(),
        ..Default::default()
    };
    let pair = record_child_result(
        &mut p.store,
        &p.run_id,
        &p.lease,
        &spawned.child_run_id,
        &s,
        &view,
        &spawned.budget_id,
    )
    .unwrap();
    assert!(!pair.result.return_contract_satisfied);
    assert!(pair
        .result
        .violations
        .iter()
        .any(|v| v.reason == "missing_required"));
    assert!(pair
        .result
        .violations
        .iter()
        .any(|v| v.reason == "claims_absent"));
}

#[test]
fn ac12_parent_stop_and_deadline_land_terminal_rows() {
    // KP-16 — parent stop cancels the live child once; the second drain is
    // a no-op.
    let mut p = open_parent("kp16-stop");
    let spawned = run(&mut p, &spec()).unwrap();
    let done = drain_on_parent_stop(&mut p.store, &p.run_id, &p.lease).unwrap();
    assert_eq!(done, vec![spawned.child_run_id.clone()]);
    let cancelled = p
        .store
        .events(&p.run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == "control.subagent.cancelled")
        .count();
    assert_eq!(cancelled, 1);
    assert_eq!(
        drain_on_parent_stop(&mut p.store, &p.run_id, &p.lease).unwrap(),
        Vec::<String>::new()
    );

    // C-4's deadline half — an unresponsive live child is terminal as
    // `infrastructure_failure{child_unresponsive}`, not silently dropped.
    let mut p2 = open_parent("kp-deadline");
    let spawned2 = run(&mut p2, &spec()).unwrap();
    let done2 = cancel_unresponsive(
        &mut p2.store,
        &p2.run_id,
        &p2.lease,
        std::slice::from_ref(&spawned2.child_run_id),
    )
    .unwrap();
    assert_eq!(done2, vec![spawned2.child_run_id]);
    let row = p2
        .store
        .events(&p2.run_id)
        .unwrap()
        .iter()
        .find(|e| e.class == "control.subagent.cancelled")
        .unwrap();
    assert_eq!(
        row.payload.get("failure_kind").and_then(Json::as_str),
        Some("child_unresponsive")
    );
}

#[test]
fn ac12_revocation_cancels_descended_children() {
    // KP-19 — revoking the parent handle marks its live children terminal;
    // a second cascade is a durable no-op.
    let mut p = open_parent("kp19");
    let spawned = run(&mut p, &spec()).unwrap();
    let done =
        cancel_on_revocation(&mut p.store, &p.run_id, &p.lease, p.handle_id.as_str()).unwrap();
    assert_eq!(done, vec![spawned.child_run_id.clone()]);
    let row = p
        .store
        .events(&p.run_id)
        .unwrap()
        .iter()
        .find(|e| e.class == "control.subagent.cancelled")
        .unwrap();
    assert_eq!(
        row.payload.get("reason").and_then(Json::as_str),
        Some("revoked")
    );
    let again =
        cancel_on_revocation(&mut p.store, &p.run_id, &p.lease, p.handle_id.as_str()).unwrap();
    assert!(again.is_empty());
}
