//! S4.8 — `lab/coordination-topology-v1` (§5e.5; ADR-0193 D6;
//! AC-R-2.6.5-7): the closed `topology` factor × `merge_policy` ×
//! `default_isolation` under `MatchSpec{matched_total}`, arm-report folds
//! (subject-charged usage, lost-write/conflict/absent counters from
//! durable merge rows), and the conditional-report gate.

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
use hh_orchestrator::coordination::{
    coordination_arm, coordination_match_report, CoordinationFactors, CoordinationTopology,
};
use hh_orchestrator::lab::{execute_arm, LabRefusal};
use hh_orchestrator::{ParentBinding, TopologyPreset, WaitPolicy};
use hh_provenance::{AuthorityClass, ProvenanceRecord};
use hh_subagent::ownership::OwnershipTable;
use hh_subagent::types::*;
use hh_wire::json::Json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

// ── fixtures (the s4_6 seam set — the lab arm runs the real spawn path) ─────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-orch-s48-{}-{tag}-{n}", std::process::id()));
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
        context: ContextIsolation::Fresh,
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

fn matched() -> MatchSpec {
    MatchSpec {
        dimensions: vec![DimensionId::ModelCalls],
        mode: MatchMode::MatchedTotal,
        tolerance_ppm: 0,
        pricing_table_ref: None,
        model_scope: ModelScope::SameSnapshot,
        cache_policy: CachePolicy::ColdStart,
        utilization_floor_ppm: None,
    }
}

// ── the closed topology factor ──────────────────────────────────────────────

#[test]
fn coordination_topology_is_closed_and_parse_only() {
    for s in [
        "single_agent",
        "orchestrator_worker_isolated",
        "orchestrator_worker_share",
        "peer_messaging",
    ] {
        assert_eq!(CoordinationTopology::parse(s).unwrap().as_str(), s);
    }
    for bad in ["swarm", "mesh", "", "SINGLE_AGENT"] {
        assert!(CoordinationTopology::parse(bad).is_none(), "{bad}");
    }
    // Only `peer_messaging` opens the sibling gate (OQ-428's default
    // `false` holds elsewhere).
    assert!(!CoordinationTopology::OrchestratorWorkerShare.requires_sibling_messaging());
    assert!(CoordinationTopology::PeerMessaging.requires_sibling_messaging());
    // The share/isolated/peer arms ride T1; `single_agent` is T0.
    assert!(matches!(
        CoordinationTopology::OrchestratorWorkerShare.preset(3),
        TopologyPreset::T1OrchestratorWorker { fan_out: 3 }
    ));
    assert!(matches!(
        CoordinationTopology::SingleAgent.preset(0),
        TopologyPreset::T0Single
    ));
}

// ── arm execution → fold → conditional match report ─────────────────────────

#[test]
fn coordination_topology_arm_executes_and_reports() {
    let mut p = open_parent("coord");
    // `single_agent` baseline arm — T0 underneath.
    let t0 = execute_arm(
        &mut p.store,
        &p.handles,
        &p.binding,
        None,
        &p.decision,
        "arm:t0:m",
        &CoordinationTopology::SingleAgent.preset(0),
        vec![],
        WaitPolicy::All,
        None,
    )
    .unwrap();
    // `orchestrator_worker_share` arm — T1 with two workers.
    let t1 = execute_arm(
        &mut p.store,
        &p.handles,
        &p.binding,
        None,
        &p.decision,
        "arm:t1:m",
        &CoordinationTopology::OrchestratorWorkerShare.preset(2),
        vec![stage_spec("a"), stage_spec("b")],
        WaitPolicy::All,
        None,
    )
    .unwrap();
    assert_eq!(t1.children.len(), 2);

    let f0 = CoordinationFactors {
        topology: CoordinationTopology::SingleAgent,
        merge_policy: MergePolicy::SingleWriter,
        default_isolation: "none".into(),
    };
    let f1 = CoordinationFactors {
        topology: CoordinationTopology::OrchestratorWorkerShare,
        merge_policy: MergePolicy::ThreeWayText,
        default_isolation: "share".into(),
    };
    let r0 = coordination_arm(&p.store, &p.binding.parent_run_id, &t0, &f0).unwrap();
    let r1 = coordination_arm(&p.store, &p.binding.parent_run_id, &t1, &f1).unwrap();
    assert!(r0.success, "T0 has no children to leave running");
    assert!(!r1.success, "the T1 children are still live");
    assert_eq!(r0.lost_write_count, 0);
    // Subject-charged usage rides every arm row.
    for r in [&r0, &r1] {
        assert_eq!(
            r.to_json().get("charged_to").and_then(Json::as_str),
            Some("subject")
        );
    }

    // The conditional-report gate — an unmatched arm refuses (never a
    // silent comparison).
    let mut execs = BTreeMap::new();
    execs.insert(r0.arm_id.clone(), &t0);
    execs.insert(r1.arm_id.clone(), &t1);
    let e = coordination_match_report(&[(&r0, None)], &execs).unwrap_err();
    assert!(matches!(e, LabRefusal::MissingMatchSpec { .. }));

    // Matched-total arms produce the `coordination_topology_v1` report.
    let spec = matched();
    let report =
        coordination_match_report(&[(&r0, Some(&spec)), (&r1, Some(&spec))], &execs).unwrap();
    assert_eq!(
        report.get("kind").and_then(Json::as_str),
        Some("coordination_topology_v1_arm_report")
    );
    if let Some(Json::Arr(arms)) = report.get("coordination_arms") {
        assert_eq!(arms.len(), 2);
        assert!(arms
            .iter()
            .all(|a| a.get("match_mode").and_then(Json::as_str) == Some("matched_total")));
        // The factors travel with the arm row — `topology`,
        // `merge_policy`, `default_isolation` are the claim's axes.
        let share_arm = arms
            .iter()
            .find(|a| a.get("arm_id").and_then(Json::as_str) == Some("arm:t1:m"))
            .unwrap();
        let factors = share_arm.get("factors").unwrap();
        assert_eq!(
            factors.get("topology").and_then(Json::as_str),
            Some("orchestrator_worker_share")
        );
        assert_eq!(
            factors.get("merge_policy").and_then(Json::as_str),
            Some("three_way_text")
        );
        assert_eq!(
            factors.get("default_isolation").and_then(Json::as_str),
            Some("share")
        );
    } else {
        panic!("coordination_arms missing from the report");
    }
}
