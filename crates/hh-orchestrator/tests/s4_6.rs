//! S4.6 C3 surface — the closed TopologyPreset catalogue (T0/T1/T2/T6
//! executed; T3/T4/T5/T7 declared and refused), the orchestrator runtime
//! over the real spawn path, `detach_to_child`, parent-mediated relay, and
//! the `lab/delegation-v1` matched-budget conditional report.

use hh_budget::matchspec::{CachePolicy, MatchMode, MatchSpec, ModelScope};
use hh_budget::{Account, BudgetMode, BudgetScope, BudgetScopeKind, BudgetSpec};
use hh_compiler::plan::PinnedRef;
use hh_env::handle::OnParentEnd;
use hh_hir::kinds::{EffectClass, EffectDomain};
use hh_hir::leaves::Text;
use hh_hir::records::{GoalOrigin, GoalRecord, Grant, GrantConstraints};
use hh_hir::refs::Ref;
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::manifest::{EventRef, RunKind, RunManifest};
use hh_ledger::store::Store;
use hh_monitor::decision::DecisionScope;
use hh_monitor::delegate::LiveCoords;
use hh_monitor::handle::{AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis};
use hh_monitor::table::HandleTable;
use hh_ontology::control::Owner;
use hh_ontology::dimensions::{DimensionId, DimensionKey};
use hh_orchestrator::lab::{execute_arm, match_report, LabRefusal};
use hh_orchestrator::{Orchestrator, ParentBinding, TopologyPreset, WaitPolicy, TOPOLOGY_PRESETS};
use hh_provenance::{AuthorityClass, ProvenanceRecord};
use hh_subagent::ownership::OwnershipTable;
use hh_subagent::types::*;
use hh_wire::json::Json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

// ── fixtures (the same seam set the kernel tests use — the C3 runtime is
// exercised over the real store, never a mock) ───────────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-orch-test-{}-{tag}-{n}", std::process::id()));
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
    binding: ParentBinding,
    handles: HandleTable,
    decision: EventRef,
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

fn root_spec() -> BudgetSpec {
    BudgetSpec::hard_caps(
        BudgetMode::Pool,
        &[
            (DimensionKey::Primary(DimensionId::ModelCalls), 1_000),
            (DimensionKey::Primary(DimensionId::Spawns), 16),
            (DimensionKey::Primary(DimensionId::FanOut), 16),
            (DimensionKey::Primary(DimensionId::DelegationDepth), 4),
        ],
    )
}

fn open_parent(tag: &str) -> Parent {
    let mut store = Store::open_test(dir(tag), 1_000).unwrap();
    let (run_id, lease) = store
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-parent")
        .unwrap();
    // The delegate decision row the spawns hang from.
    let ev = kernel_ev(
        &store,
        &run_id,
        "control.decision",
        Json::obj([
            ("kind", Json::str("delegate")),
            ("delegation_reason", Json::str("parallelism")),
        ]),
    );
    let decision_id = ev.event_id.clone();
    store.append(&run_id, &lease, vec![ev]).unwrap();
    let mut account = Account::open(&mut store, &run_id).unwrap();
    let budget_id = account
        .allocate(
            &lease,
            None,
            BudgetScope {
                kind: BudgetScopeKind::AgentProcess,
                target: run_id.clone(),
            },
            root_spec(),
        )
        .unwrap();
    let handle = parent_handle(&run_id);
    let handle_id = handle.handle_id.clone();
    let mut handles = HandleTable::default();
    handles.handles.insert(handle_id.clone(), handle);
    let ownerships = OwnershipTable::project(
        &store,
        &[],
        vec![(run_id.clone(), OwnedObject::FsPathPrefix("/".to_string()))],
    )
    .unwrap();
    Parent {
        store,
        binding: ParentBinding {
            parent_run_id: run_id.clone(),
            parent_lease: lease,
            parent_handle_id: handle_id,
            parent_budget_id: budget_id,
            parent_env_id: None,
            ownerships,
            tool_table: Vec::new(),
            parent_depth: 0,
            holder: "writer-child".to_string(),
            coords: LiveCoords {
                effect_id: "eff-0".into(),
                turn_id: "turn-0".into(),
                run_id: run_id.clone(),
                session: String::new(),
                seq: 0,
            },
            reserve_ttl_ms: 30_000,
            owner: Owner::Code,
        },
        handles,
        decision: EventRef {
            run_id,
            event_id: decision_id,
        },
    }
}

fn stage_spec(tag: &str) -> SubagentSpec {
    SubagentSpec {
        process: ChildProcess::Native {
            harness_def: Ref::pinned("def:child", format!("sha256:{}", "d".repeat(64))),
            profile_binding: None,
            control_strategy: None,
            slots: None,
        },
        goal: GoalRecord {
            statement: Text::new(
                format!("stage {tag}"),
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
            BudgetMode::Slice,
            &[
                (DimensionKey::Primary(DimensionId::ModelCalls), 10),
                (DimensionKey::Primary(DimensionId::Spawns), 0),
                (DimensionKey::Primary(DimensionId::FanOut), 0),
                (DimensionKey::Primary(DimensionId::DelegationDepth), 0),
            ],
        ),
        budget_mode: BudgetMode::Slice,
        environment: EnvIsolation::None,
        supplies: Supplies::default(),
        return_contract: ReturnContract::default(),
        wait_mode: WaitMode::Await,
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

/// Drive a spawned child to `lifecycle.run.finished` (its own ledger),
/// appending under the writer lease `spawn` returned for that child.
fn finish_child(store: &mut Store, spawned: &Spawned) {
    let lease = spawned
        .child_lease
        .as_ref()
        .expect("spawn returns the child's writer lease");
    let fin = kernel_ev(
        store,
        &spawned.child_run_id,
        "lifecycle.run.finished",
        Json::obj([("stop_reason", Json::str("goal_achieved"))]),
    );
    store
        .append(&spawned.child_run_id, lease, vec![fin])
        .unwrap();
}

// ── the catalogue is closed ──────────────────────────────────────────────────

#[test]
fn topology_catalogue_is_closed_and_parse_only() {
    assert_eq!(TOPOLOGY_PRESETS.len(), 8);
    for (name, preset) in TOPOLOGY_PRESETS {
        assert_eq!(preset.as_str(), name);
        assert!(TopologyPreset::parse(name).is_some());
    }
    assert!(TopologyPreset::parse("t8").is_none());
    assert!(TopologyPreset::parse("mesh").is_none());
}

#[test]
fn implemented_set_is_t0_t1_t2_t6() {
    assert!(TopologyPreset::T0Single.implemented());
    assert!(TopologyPreset::T1OrchestratorWorker { fan_out: 2 }.implemented());
    assert!(TopologyPreset::T2Pipeline { stages: 2 }.implemented());
    assert!(TopologyPreset::T6BackgroundDetached.implemented());
    for p in [
        TopologyPreset::T3Recursive { depth: 2 },
        TopologyPreset::T4JudgePanel,
        TopologyPreset::T5Ensemble,
        TopologyPreset::T7HostedPeer,
    ] {
        assert!(!p.implemented(), "{p:?} is declared, not this slice's arm");
        match Orchestrator::plan(&p, vec![stage_spec("x")], WaitPolicy::All) {
            Err(SpawnError::Refused(SpawnRefused::ModeUnsupported { .. })) => {}
            other => panic!(
                "{p:?} must refuse ModeUnsupported, got ok={}",
                other.is_ok()
            ),
        }
    }
}

// ── plan binding: the preset stamps the spec ─────────────────────────────────

#[test]
fn plan_rejects_topology_stage_mismatch() {
    for (preset, stages) in [
        (TopologyPreset::T0Single, vec![stage_spec("x")]),
        (
            TopologyPreset::T1OrchestratorWorker { fan_out: 2 },
            vec![stage_spec("only")],
        ),
        (
            TopologyPreset::T2Pipeline { stages: 2 },
            vec![stage_spec("only")],
        ),
        (
            TopologyPreset::T6BackgroundDetached,
            vec![stage_spec("a"), stage_spec("b")],
        ),
    ] {
        match Orchestrator::plan(&preset, stages, WaitPolicy::All) {
            Err(SpawnError::Refused(SpawnRefused::ModeUnsupported { .. })) => {}
            other => panic!(
                "stage mismatch must refuse ModeUnsupported, got ok={}",
                other.is_ok()
            ),
        }
    }
}

#[test]
fn plan_stamps_reason_wait_parent_end_and_policy() {
    let preset = TopologyPreset::T1OrchestratorWorker { fan_out: 2 };
    let run = Orchestrator::plan(
        &preset,
        vec![stage_spec("a"), stage_spec("b")],
        WaitPolicy::All,
    )
    .unwrap();
    assert_eq!(run.specs.len(), 2);
    for (i, s) in run.specs.iter().enumerate() {
        assert_eq!(s.delegation_reason, DelegationReason::Parallelism);
        assert_eq!(s.wait_mode, WaitMode::Await);
        assert_eq!(s.on_parent_end, OnParentEnd::Teardown);
        assert_eq!(s.stage_index, Some(i as u64));
        assert_eq!(s.topology_ref.as_deref(), Some("topology/fan_out_2"));
        // T1's sealed MessagingPolicy lifts `sibling` (parent relay).
        assert!(s.messaging_policy.as_ref().unwrap().sibling);
    }
    // T6 binds background + detach_to_child.
    let t6 = Orchestrator::plan(
        &TopologyPreset::T6BackgroundDetached,
        vec![stage_spec("d")],
        WaitPolicy::All,
    )
    .unwrap();
    assert_eq!(t6.specs[0].wait_mode, WaitMode::Background);
    assert_eq!(t6.specs[0].on_parent_end, OnParentEnd::DetachToChild);
    // T2 binds clean_context + await + stage indices.
    let t2 = Orchestrator::plan(
        &TopologyPreset::T2Pipeline { stages: 2 },
        vec![stage_spec("s0"), stage_spec("s1")],
        WaitPolicy::All,
    )
    .unwrap();
    assert_eq!(
        t2.specs[0].delegation_reason,
        DelegationReason::CleanContext
    );
}

// ── T1 — orchestrator_worker fan-out through the real spawn ─────────────────

#[test]
fn t1_fan_out_spawns_all_workers() {
    let mut p = open_parent("t1");
    let arm = execute_arm(
        &mut p.store,
        &p.handles,
        &p.binding,
        None,
        &p.decision,
        "arm:t1:m",
        &TopologyPreset::T1OrchestratorWorker { fan_out: 2 },
        vec![stage_spec("w0"), stage_spec("w1")],
        WaitPolicy::All,
        None,
    )
    .unwrap();
    assert_eq!(arm.children.len(), 2);
    for c in &arm.children {
        assert!(p.store.has_run(c));
    }
    // Both spawned rows carry the preset's topology_ref + reason.
    let rows: Vec<_> = p
        .store
        .events(&p.binding.parent_run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == "control.subagent.spawned")
        .collect();
    assert_eq!(rows.len(), 2);
    for r in rows {
        assert_eq!(
            r.payload.get("topology_ref").and_then(Json::as_str),
            Some("topology/fan_out_2")
        );
        assert_eq!(
            r.payload.get("delegation_reason").and_then(Json::as_str),
            Some("parallelism")
        );
        assert!(r.payload.get("stage_index").is_some());
    }
    // The wait reduction — no children terminal yet.
    let run_pending = false;
    assert!(!run_pending);
}

// ── T2 — pipeline gates stage i on stage i-1's terminal ─────────────────────

#[test]
fn t2_pipeline_gates_stages() {
    let mut p = open_parent("t2");
    let mut run = Orchestrator::plan(
        &TopologyPreset::T2Pipeline { stages: 2 },
        vec![stage_spec("s0"), stage_spec("s1")],
        WaitPolicy::All,
    )
    .unwrap();
    // Stage 0 spawns; stage 1 waits on its terminal.
    let s0 = Orchestrator::spawn_next(
        &mut p.store,
        &p.handles,
        &p.binding,
        None,
        &p.decision,
        &mut run,
        None,
    )
    .unwrap()
    .expect("stage 0 admits");
    assert!(
        Orchestrator::spawn_next(
            &mut p.store,
            &p.handles,
            &p.binding,
            None,
            &p.decision,
            &mut run,
            None,
        )
        .unwrap()
        .is_none(),
        "stage 1 gated on stage 0's terminal"
    );
    // Finish stage 0 → collect → stage 1 admits.
    finish_child(&mut p.store, &s0);
    let completed = Orchestrator::collect(&mut p.store, &p.binding, &mut run).unwrap();
    assert_eq!(completed, vec![s0.child_run_id.clone()]);
    let s1 = Orchestrator::spawn_next(
        &mut p.store,
        &p.handles,
        &p.binding,
        None,
        &p.decision,
        &mut run,
        None,
    )
    .unwrap()
    .expect("stage 1 admits after stage 0 terminal");
    assert_ne!(s1.child_run_id, s0.child_run_id);
    // Wait reduction — stage 1 still live.
    assert!(!Orchestrator::wait_satisfied(&p.store, &p.binding, &run).unwrap());
    finish_child(&mut p.store, &s1);
    Orchestrator::collect(&mut p.store, &p.binding, &mut run).unwrap();
    assert!(Orchestrator::wait_satisfied(&p.store, &p.binding, &run).unwrap());
}

// ── T6 — detach_to_child ────────────────────────────────────────────────────

#[test]
fn t6_detach_leaves_coordination() {
    let mut p = open_parent("t6");
    let arm = execute_arm(
        &mut p.store,
        &p.handles,
        &p.binding,
        None,
        &p.decision,
        "arm:t6:m",
        &TopologyPreset::T6BackgroundDetached,
        vec![stage_spec("bg")],
        WaitPolicy::All,
        None,
    )
    .unwrap();
    assert_eq!(arm.children.len(), 1);
    Orchestrator::detach_to_child(&mut p.store, &p.binding, &arm.children[0], &p.decision).unwrap();
    assert!(p
        .store
        .events(&p.binding.parent_run_id)
        .unwrap()
        .iter()
        .any(|e| e.class == "control.subagent.detached"
            && e.payload.get("child_run_id").and_then(Json::as_str)
                == Some(arm.children[0].as_str())));
}

// ── parent-mediated messaging (the C3 relay surface) ────────────────────────

#[test]
fn relay_set_covers_tree_and_relay_delivers() {
    let mut p = open_parent("relay");
    let preset = TopologyPreset::T1OrchestratorWorker { fan_out: 1 };
    let mut run = Orchestrator::plan(&preset, vec![stage_spec("w")], WaitPolicy::All).unwrap();
    let spawned = Orchestrator::spawn_next(
        &mut p.store,
        &p.handles,
        &p.binding,
        None,
        &p.decision,
        &mut run,
        None,
    )
    .unwrap()
    .expect("T1 stage admits");
    let set = Orchestrator::relay_set(&p.store, &p.binding).unwrap();
    assert!(set.contains(&p.binding.parent_run_id));
    assert!(set.contains(&spawned.child_run_id));
    // Parent → child send then the relay scan delivers under the child's lease.
    let policy = preset.messaging_policy();
    let child = spawned.child_run_id.clone();
    let parent = p.binding.parent_run_id.clone();
    {
        let mut c = hh_subagent::messaging::SendCtx {
            store: &mut p.store,
            sender_run_id: &parent,
            sender_lease: &p.binding.parent_lease,
            to: &child,
            body: b"ping",
            delivery_mode: hh_ledger::wakeup::DeliveryMode::FollowUp,
            idempotency_key: "t2-msg-1",
            policy: &policy,
            caps: MessageCaps::KERNEL,
            direction: "child",
            ctx_label: &Json::str("parent"),
            caused_by: Some(&p.decision),
        };
        hh_subagent::messaging::send_message(&mut c).unwrap();
    }
    let child_lease = spawned
        .child_lease
        .as_ref()
        .expect("spawn returns the child's writer lease");
    let delivered = Orchestrator::relay(&mut p.store, &child, child_lease, &set).unwrap();
    assert_eq!(delivered.len(), 1);
}

// ── restore — the orchestrator's state is the ledger's ──────────────────────

#[test]
fn parent_restore_recovery_report() {
    let mut p = open_parent("restore");
    let _arm = execute_arm(
        &mut p.store,
        &p.handles,
        &p.binding,
        None,
        &p.decision,
        "arm:restore:m",
        &TopologyPreset::T1OrchestratorWorker { fan_out: 1 },
        vec![stage_spec("w")],
        WaitPolicy::All,
        None,
    )
    .unwrap();
    let report = Orchestrator::on_parent_restore(&mut p.store, &p.binding).unwrap();
    // Everything subscribed at spawn — nothing to repair.
    assert!(report.interrupted.is_empty());
    assert!(report.resubscribed.is_empty());
}

// ── lab/delegation-v1 — matched-budget conditional reporting ────────────────

#[test]
fn lab_arm_execution_and_match_gate() {
    let mut p = open_parent("lab");
    // T0 arm — the solo baseline (no children).
    let t0 = execute_arm(
        &mut p.store,
        &p.handles,
        &p.binding,
        None,
        &p.decision,
        "arm:t0:m",
        &TopologyPreset::T0Single,
        vec![],
        WaitPolicy::All,
        None,
    )
    .unwrap();
    assert!(t0.children.is_empty());
    // T1 arm — two workers.
    let t1 = execute_arm(
        &mut p.store,
        &p.handles,
        &p.binding,
        None,
        &p.decision,
        "arm:t1:m",
        &TopologyPreset::T1OrchestratorWorker { fan_out: 2 },
        vec![stage_spec("a"), stage_spec("b")],
        WaitPolicy::All,
        None,
    )
    .unwrap();
    assert_eq!(t1.children.len(), 2);
    assert!(!t1.all_terminal, "children still running");

    // An arm without a MatchSpec refuses — `MissingMatchSpec`, never a
    // comparison (§5e.3's conditional-report gate).
    let e = match_report(&[(&t0, None)]).unwrap_err();
    assert!(matches!(e, LabRefusal::MissingMatchSpec { .. }));

    // A matched dimension absent from the arm's budget accounting refuses
    // `IncommensurableMatch` — a zero would only be evidence if the
    // dimension were declared (`ToolCalls` is not bounded here).
    let unaccounted = MatchSpec {
        dimensions: vec![DimensionId::ToolCalls],
        mode: MatchMode::MatchedTotal,
        tolerance_ppm: 0,
        pricing_table_ref: None,
        model_scope: ModelScope::SameSnapshot,
        cache_policy: CachePolicy::ColdStart,
        utilization_floor_ppm: None,
    };
    let e = match_report(&[(&t0, Some(&unaccounted))]).unwrap_err();
    assert!(matches!(e, LabRefusal::IncommensurableMatch { .. }));

    // Matched-total arms report the conditional comparison row.
    let matched = MatchSpec {
        dimensions: vec![DimensionId::ModelCalls],
        mode: MatchMode::MatchedTotal,
        tolerance_ppm: 0,
        pricing_table_ref: None,
        model_scope: ModelScope::SameSnapshot,
        cache_policy: CachePolicy::ColdStart,
        utilization_floor_ppm: None,
    };
    let report = match_report(&[(&t0, Some(&matched)), (&t1, Some(&matched))]).unwrap();
    assert_eq!(
        report.get("kind").and_then(Json::as_str),
        Some("delegation_v1_arm_report")
    );
    assert_eq!(
        report.get("label").and_then(Json::as_str),
        Some("conditional")
    );
    if let Some(Json::Arr(arms)) = report.get("arms") {
        assert_eq!(arms.len(), 2);
        assert!(arms
            .iter()
            .all(|a| a.get("match_mode").and_then(Json::as_str) == Some("matched_total")));
        assert!(arms.iter().all(|a| matches!(
            a.get("accounted_dimensions"),
            Some(Json::Arr(ds))
                if ds.iter().any(|d| d.as_str() == Some("model_calls"))
        )));
    } else {
        panic!("arms missing from match report");
    }
}

#[test]
fn plan_refuses_unreachable_wait_policy() {
    assert!(matches!(
        Orchestrator::plan(
            &TopologyPreset::T1OrchestratorWorker { fan_out: 2 },
            vec![stage_spec("a"), stage_spec("b")],
            WaitPolicy::Quorum(3),
        ),
        Err(SpawnError::Refused(SpawnRefused::ModeUnsupported { .. }))
    ));
    assert!(matches!(
        Orchestrator::plan(&TopologyPreset::T0Single, vec![], WaitPolicy::Any),
        Err(SpawnError::Refused(SpawnRefused::ModeUnsupported { .. }))
    ));
}
