//! S5.5 — T3 `recursive` admitted (§5e.3; R-2.6.3 Stage-5 slice): the
//! recursion binds against each child's `delegation_depth` ceiling at
//! `plan`, `share`-mode children spawn through the kernel seam (step-4c
//! `resource(key)` locks + K-2 `require_hlc`), the spawned rows carry
//! `depth` (the child `delegation_depth` gauge), and the recursion is
//! real — a spawned child orchestrates its own T3 arm at `depth − 1`.
//! The orchestrator's spawn decision is `owner = code` (the `delegate`
//! is the scheduler/sealed decision's, never a `model_claim`).

use hh_budget::{Account, BudgetMode, BudgetScope, BudgetScopeKind, BudgetSpec};
use hh_compiler::plan::PinnedRef;
use hh_env::handle::{DeriveMode, OnParentEnd};
use hh_hir::kinds::{EffectClass, EffectDomain};
use hh_hir::leaves::Text;
use hh_hir::records::{GoalOrigin, GoalRecord, Grant, GrantConstraints};
use hh_hir::refs::Ref;
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::manifest::{EventRef, RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_monitor::decision::DecisionScope;
use hh_monitor::delegate::LiveCoords;
use hh_monitor::handle::{AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis};
use hh_monitor::table::HandleTable;
use hh_ontology::control::Owner;
use hh_ontology::dimensions::{DimensionId, DimensionKey};
use hh_orchestrator::{Orchestrator, ParentBinding, TopologyPreset, WaitPolicy};
use hh_provenance::{AuthorityClass, ProvenanceRecord};
use hh_subagent::consistency::{ConsistencyDeclaration, ConsistencyLevel};
use hh_subagent::ownership::OwnershipTable;
use hh_subagent::spawn::EnvDeriver;
use hh_subagent::types::*;
use hh_wire::json::Json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

// ── fixtures (the s4_8 seam set) ────────────────────────────────────────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-orch-s55-{}-{tag}-{n}", std::process::id()));
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
        handle_id: HandleId(format!("hnd-{run_id}")),
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

/// Open a parent run at `delegation_depth = depth` — the recursion test
/// re-binds a spawned child as its own T3 parent.
fn open_parent_at(_tag: &str, store: Store, run_id: String, lease: &Lease, depth: u64) -> Parent {
    let mut store = store;
    let ev = kernel_ev(
        &store,
        &run_id,
        "control.decision",
        Json::obj([
            ("kind", Json::str("delegate")),
            ("delegation_reason", Json::str("specialization")),
            ("owner", Json::str("code")),
        ]),
    );
    let decision_id = ev.event_id.clone();
    store.append(&run_id, lease, vec![ev]).unwrap();
    let mut account = Account::open(&mut store, &run_id).unwrap();
    let budget_id = account
        .allocate(
            lease,
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
            parent_lease: lease.clone(),
            parent_handle_id: handle_id,
            parent_budget_id: budget_id,
            parent_env_id: None,
            ownerships,
            tool_table: Vec::new(),
            parent_depth: depth,
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
        run_id: run_id.clone(),
        decision: EventRef {
            run_id,
            event_id: decision_id,
        },
    }
}

/// Re-bind a spawned child run as its own T3 parent — the recursion
/// binds a fresh `ParentBinding` over the *same* store (the child's
/// `delegation_depth` gauge reads `depth` — its own `parent_depth`).
fn bind_child_parent(
    store: &mut Store,
    child_run_id: &str,
    lease: &Lease,
    depth: u64,
) -> (ParentBinding, HandleTable, EventRef) {
    let ev = kernel_ev(
        store,
        child_run_id,
        "control.decision",
        Json::obj([
            ("kind", Json::str("delegate")),
            ("delegation_reason", Json::str("specialization")),
            ("owner", Json::str("code")),
        ]),
    );
    let decision_id = ev.event_id.clone();
    store.append(child_run_id, lease, vec![ev]).unwrap();
    let mut account = Account::open(store, child_run_id).unwrap();
    let budget_id = account
        .allocate(
            lease,
            None,
            BudgetScope {
                kind: BudgetScopeKind::AgentProcess,
                target: child_run_id.to_string(),
            },
            root_spec(),
        )
        .unwrap();
    let handle = parent_handle(child_run_id);
    let handle_id = handle.handle_id.clone();
    let mut handles = HandleTable::default();
    handles.handles.insert(handle_id.clone(), handle);
    let ownerships = OwnershipTable::project(
        store,
        &[],
        vec![(
            child_run_id.to_string(),
            OwnedObject::FsPathPrefix("/".to_string()),
        )],
    )
    .unwrap();
    (
        ParentBinding {
            parent_run_id: child_run_id.to_string(),
            parent_lease: lease.clone(),
            parent_handle_id: handle_id,
            parent_budget_id: budget_id,
            parent_env_id: None,
            ownerships,
            tool_table: Vec::new(),
            parent_depth: depth,
            holder: "writer-child".to_string(),
            coords: LiveCoords {
                effect_id: "eff-c".into(),
                turn_id: "turn-c".into(),
                run_id: child_run_id.to_string(),
                session: String::new(),
                seq: 0,
            },
            reserve_ttl_ms: 30_000,
            owner: Owner::Code,
        },
        handles,
        EventRef {
            run_id: child_run_id.to_string(),
            event_id: decision_id,
        },
    )
}

fn open_parent(tag: &str) -> Parent {
    let mut store = Store::open_test(dir(tag), 1_000).unwrap();
    let (run_id, lease) = store
        .open_run(RunManifest::minimal(RunKind::Agent), "writer-parent")
        .unwrap();
    open_parent_at(tag, store, run_id, &lease, 0)
}

/// A T3 stage spec — `delegation_depth` ceiling `d` admits `d` further
/// orchestration levels (the preset's declared recursion binds against
/// it at `plan`).
fn stage_spec(tag: &str, depth_cap: i64) -> SubagentSpec {
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
                (DimensionKey::Primary(DimensionId::Spawns), 4),
                (DimensionKey::Primary(DimensionId::FanOut), 4),
                (
                    DimensionKey::Primary(DimensionId::DelegationDepth),
                    depth_cap,
                ),
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

/// A `share`-mode T3 stage — `resource(key)` reservations + the
/// K-1-consistent `causal_cross_run` declaration (share never declares
/// `snapshot_at_fork`).
fn share_stage(tag: &str, depth_cap: i64, keys: &[&str]) -> SubagentSpec {
    let mut s = stage_spec(tag, depth_cap);
    s.environment = EnvIsolation::Derive {
        mode: DeriveMode::Share,
        spec: Json::obj([("env_class", Json::str("workspace"))]),
    };
    s.reserved_keys = keys.iter().map(|k| k.to_string()).collect();
    s.consistency_declarations = vec![ConsistencyDeclaration {
        object: None,
        kind_wildcard: Some("fs".to_string()),
        level: ConsistencyLevel::CausalCrossRun,
        staleness_bound_ms: None,
        readers: "family".to_string(),
    }
    .to_json()];
    s
}

/// The derive port double — `share` arms need a live parent env + driver
/// (the durable `derived` row is the parent's, under its lease).
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

// ── plan-time binding ───────────────────────────────────────────────────────

#[test]
fn t3_plan_binds_children_with_the_declared_reason() {
    let preset = TopologyPreset::T3Recursive { depth: 2 };
    assert!(preset.implemented(), "T3 is admitted at S5.5");
    let run = Orchestrator::plan(
        &preset,
        vec![stage_spec("a", 1), stage_spec("b", 1)],
        WaitPolicy::All,
    )
    .unwrap();
    for (i, s) in run.specs.iter().enumerate() {
        assert_eq!(s.topology_ref.as_deref(), Some("topology/depth_2"));
        assert_eq!(s.delegation_reason, DelegationReason::Specialization);
        assert_eq!(s.wait_mode, WaitMode::Await);
        assert_eq!(s.on_parent_end, OnParentEnd::Teardown);
        assert_eq!(s.stage_index, Some(i as u64));
    }
}

#[test]
fn t3_plan_refuses_depth_zero() {
    let e = match Orchestrator::plan(
        &TopologyPreset::T3Recursive { depth: 0 },
        vec![stage_spec("a", 1)],
        WaitPolicy::All,
    ) {
        Err(e) => e,
        Ok(_) => panic!("depth 0 refuses"),
    };
    assert!(matches!(
        e,
        SpawnError::Refused(SpawnRefused::ModeUnsupported { .. })
    ));
}

#[test]
fn t3_plan_binds_depth_against_child_depth_caps() {
    // A `depth = 2` arm's children orchestrate one further level — a
    // child spec whose `delegation_depth` ceiling is 0 cannot carry the
    // declared recursion (typed `Depth` refusal at `plan`, never a
    // runtime surprise).
    let e = match Orchestrator::plan(
        &TopologyPreset::T3Recursive { depth: 2 },
        vec![stage_spec("a", 0)],
        WaitPolicy::All,
    ) {
        Err(e) => e,
        Ok(_) => panic!("an under-capped child refuses the declared depth"),
    };
    assert!(matches!(
        e,
        SpawnError::Refused(SpawnRefused::Depth { depth: 2, cap: 0 })
    ));
    // `depth = 1` (a leaf level) binds against cap 0 — no further level
    // is owed.
    assert!(Orchestrator::plan(
        &TopologyPreset::T3Recursive { depth: 1 },
        vec![stage_spec("a", 0)],
        WaitPolicy::All,
    )
    .is_ok());
}

// ── spawned children: share mode + the depth gauge ─────────────────────────

#[test]
fn t3_share_children_spawn_through_the_kernel_seam() {
    let mut p = open_parent("share");
    let preset = TopologyPreset::T3Recursive { depth: 2 };
    let mut run = Orchestrator::plan(
        &preset,
        vec![
            share_stage("a", 1, &["src/a"]),
            share_stage("b", 1, &["src/b"]),
        ],
        WaitPolicy::All,
    )
    .unwrap();
    let mut drv = TestDeriver;
    // The parent's env handle exists for the derive.
    p.binding.parent_env_id = Some("env-parent".into());
    let mut spawned_ids = Vec::new();
    for _ in 0..2 {
        let spawned = Orchestrator::spawn_next(
            &mut p.store,
            &p.handles,
            &p.binding,
            Some(&mut drv),
            &p.decision,
            &mut run,
            None,
        )
        .unwrap()
        .expect("a child spawns");
        spawned_ids.push(spawned.child_run_id.clone());
    }
    // The spawned rows carry `depth = parent_depth + 1`, `topology t3`,
    // `reserved_keys` and the code-owned delegation label — share
    // children run under K-2's mandatory HLC (the parent's subsequent
    // rows stamp `hlc`).
    let rows: Vec<_> = p
        .store
        .events(&p.run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == "control.subagent.spawned")
        .cloned()
        .collect();
    assert_eq!(rows.len(), 2);
    for (i, r) in rows.iter().enumerate() {
        assert_eq!(r.payload.get("depth").and_then(Json::as_int), Some(1));
        assert_eq!(
            r.payload.get("topology_ref").and_then(Json::as_str),
            Some("topology/depth_2")
        );
        assert_eq!(
            r.payload.get("owner").and_then(Json::as_str),
            Some("code"),
            "the spawn decision is code-owned — never a model_claim"
        );
        let keys = match r.payload.get("reserved_keys") {
            Some(Json::Arr(k)) => k.clone(),
            _ => panic!("reserved_keys ledgered"),
        };
        assert_eq!(keys.len(), 1);
        assert_eq!(spawned_ids[i], *r.scope.child_run_id.as_ref().unwrap());
    }
    // K-2 — the parent's post-spawn rows carry `hlc`.
    let last = p.store.events(&p.run_id).unwrap().last().unwrap().clone();
    assert!(last.hlc.is_some(), "share children arm the parent's HLC");
}

#[test]
fn t3_recursion_reaches_depth_two_through_real_runs() {
    let mut p = open_parent("rec");
    // depth_cap ≥ 2: parent at depth 0 plans T3{2} — the child spec
    // declares `delegation_depth` capacity 1 so it can orchestrate the
    // second level itself.
    let mut run = Orchestrator::plan(
        &TopologyPreset::T3Recursive { depth: 2 },
        vec![stage_spec("a", 1)],
        WaitPolicy::All,
    )
    .unwrap();
    let child = Orchestrator::spawn_next(
        &mut p.store,
        &p.handles,
        &p.binding,
        None,
        &p.decision,
        &mut run,
        None,
    )
    .unwrap()
    .expect("the depth-1 child");
    // The child run exists; it orchestrates T3{depth:1} — its own spawn
    // binds at `delegation_depth = 2` (parent_depth 1 + 1 ≤ its cap).
    let child_lease = child.child_lease.clone().expect("child lease");
    let (cb, chandles, cdec) =
        bind_child_parent(&mut p.store, &child.child_run_id, &child_lease, 1);
    let mut run2 = match Orchestrator::plan(
        &TopologyPreset::T3Recursive { depth: 1 },
        vec![stage_spec("g", 0)],
        WaitPolicy::All,
    ) {
        Ok(r) => r,
        Err(e) => panic!("the child's T3 plan binds: {e:?}"),
    };
    let grandchild =
        Orchestrator::spawn_next(&mut p.store, &chandles, &cb, None, &cdec, &mut run2, None)
            .unwrap()
            .expect("the depth-2 grandchild");
    let spawned_row = p
        .store
        .events(&child.child_run_id)
        .unwrap()
        .iter()
        .find(|e| e.class == "control.subagent.spawned")
        .cloned()
        .expect("the child's spawned row");
    assert_eq!(
        spawned_row.payload.get("depth").and_then(Json::as_int),
        Some(2),
        "the grandchild lands at delegation_depth 2"
    );
    assert_eq!(
        p.store
            .manifest(&grandchild.child_run_id)
            .unwrap()
            .parent_run_id
            .as_deref(),
        Some(child.child_run_id.as_str())
    );
    // A third level refuses the gauge — depth 3 exceeds the child's
    // slice cap (delegation_depth ceiling 1 on the spawned child spec).
    let e = match Orchestrator::plan(
        &TopologyPreset::T3Recursive { depth: 2 },
        vec![stage_spec("h", 0)],
        WaitPolicy::All,
    ) {
        Err(e) => e,
        Ok(_) => panic!("the leaf-level spec cannot recurse"),
    };
    assert!(matches!(e, SpawnError::Refused(SpawnRefused::Depth { .. })));
}
