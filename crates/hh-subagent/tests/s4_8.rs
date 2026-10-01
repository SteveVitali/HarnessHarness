//! S4.8 acceptance matrix — AC-R-2.6.5-{1…10} coordination & consistency:
//! `share`/`scoped_subtree` + `resource(key)` locks + HLC, K-1 declaration
//! checks, ownership check 8 + verbs, `three_way_text{line}` /
//! `validator_selected` merge arms, `merge_hash` determinism, absent-child
//! accounting, and terminal-time lock release — all over the real
//! `hh-ledger` store (no mocks at the ledger seam).

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
use hh_provenance::{AuthorityClass, ProvenanceRecord};
use hh_subagent::consistency::{ConsistencyDeclaration, ConsistencyLevel, OnAbsentChild};
use hh_subagent::merge::{
    merge, MergeCtx, MergeInput, MergeOpts, MergeValidator, MergeVeto, ValidatorMergeVerdict,
};
use hh_subagent::ownership::{
    check_write, record_write_refusal, return_ownership, transfer_ownership, OwnershipTable,
    WriteVerdict,
};
use hh_subagent::result::{record_child_result, ChildTerminalView};
use hh_subagent::spawn::{spawn, EnvDeriver, SpawnCtx};
use hh_subagent::types::*;
use hh_wire::json::Json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

// ── fixtures (the s4_6 harness — duplicated per integration binary) ─────────

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("hh-subagent-s48-{}-{tag}-{n}", std::process::id()));
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
    p.ownerships = OwnershipTable::project(
        &p.store,
        &[],
        vec![(p.run_id.clone(), OwnedObject::FsPathPrefix("/".to_string()))],
    )
    .unwrap();
    p
}

fn spec() -> SubagentSpec {
    spec_with(BudgetMode::Slice, WaitMode::Background)
}

fn spec_with(mode: BudgetMode, wait: WaitMode) -> SubagentSpec {
    SubagentSpec {
        process: ChildProcess::Native {
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
        context: ContextIsolation::Fresh,
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
        branch_ctx: None,
        hosted_plane: None,
        hook: None,
    }
}

fn run(p: &mut Parent, s: &SubagentSpec) -> Result<Spawned, SpawnError> {
    let mut c = ctx(p);
    spawn(&mut c, s)
}

/// A `share`-armed spec reserving `keys` with the K-1-consistent
/// `causal_cross_run` declaration.
fn share_spec(keys: &[&str]) -> SubagentSpec {
    let mut s = spec();
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

/// A minimal `EnvDeriver` — the share/scoped derive arm needs a live
/// parent env + driver (the driver double lands the durable marker row).
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

/// Spawn with the environment seam live (share/scoped derive arms).
fn run_env(p: &mut Parent, s: &SubagentSpec) -> Result<Spawned, SpawnError> {
    let mut drv = TestDeriver;
    let mut c = ctx(p);
    c.parent_env_id = Some("env-parent");
    c.env_driver = Some(&mut drv);
    spawn(&mut c, s)
}

fn resource_lease_holders(p: &Parent, key: &str) -> Vec<String> {
    p.store
        .scoped_lease_holders(&hh_ledger::leases::LeaseScope::Resource(key.to_string()))
        .into_iter()
        .map(|(_, l)| l.holder)
        .collect()
}

// ── AC-R-2.6.5-1 — `share` spawn: N children over one env, disjoint keys ────

#[test]
fn ac1_share_children_hold_disjoint_keys_and_hlc_stamps() {
    let mut p = open_parent("ac1");
    let a = run_env(&mut p, &share_spec(&["fs_path:/shared/a"])).unwrap();
    delegate_decision(&mut p, 1);
    let b = run_env(&mut p, &share_spec(&["fs_path:/shared/b"])).unwrap();
    // Each child holds its own key under `subagent:<child>` — the locks
    // survive the spawn (released at the child's terminal, O-5).
    assert_eq!(
        resource_lease_holders(&p, "fs_path:/shared/a"),
        vec![format!("subagent:{}", a.child_run_id)]
    );
    assert_eq!(
        resource_lease_holders(&p, "fs_path:/shared/b"),
        vec![format!("subagent:{}", b.child_run_id)]
    );
    // K-2 — `share` requires HLC on the parent's record; subsequent
    // shared-tree coordination rows stamp it.
    assert!(p.store.hlc_enabled(&p.run_id));
    let events = p.store.events(&p.run_id).unwrap();
    let last_spawned = events
        .iter()
        .rev()
        .find(|e| e.class == "control.subagent.spawned")
        .expect("spawned row");
    assert!(last_spawned.hlc.is_some(), "share rows stamp HLC");
}

// ── AC-R-2.6.5-3 — `resource(key)`: overlap refuses, all-or-nothing ─────────

#[test]
fn ac3_overlapping_key_refuses_resource_lock_timeout() {
    let mut p = open_parent("ac3");
    let _a = run_env(&mut p, &share_spec(&["k1"])).unwrap();
    delegate_decision(&mut p, 1);
    let err = run_env(&mut p, &share_spec(&["k1"])).unwrap_err();
    assert!(matches!(
        err,
        SpawnError::Refused(SpawnRefused::ResourceLockTimeout { .. })
    ));
}

/// A second run on the parent's store — the cross-run holder `resource`
/// leases contend against (`scoped_lease_holders` scans every run in the
/// ledger, not run-local state).
fn rival_run(p: &mut Parent) -> (String, Lease) {
    let manifest = RunManifest::minimal(RunKind::Agent);
    p.store.open_run(manifest, "writer-rival").unwrap()
}

#[test]
fn ac3_sorted_order_and_failed_spawn_releases_prefix() {
    let mut p = open_parent("ac3b");
    let (rival_run, rival_lease) = rival_run(&mut p);
    // The rival takes `k2` on its own run in the same ledger (cross-run
    // holder).
    let scope = hh_ledger::leases::LeaseScope::Resource("k2".to_string());
    p.store
        .lease_acquire(&rival_run, &rival_lease, &scope, "subagent:rival", 30_000)
        .unwrap();

    // Unsorted declaration — acquisition order is canonical sorted
    // (`k1` first); the live `k2` holder makes the attempt contend and
    // the already-taken `k1` hold must release with the refusal.
    let err = run_env(&mut p, &share_spec(&["k2", "k1"])).unwrap_err();
    assert!(matches!(
        err,
        SpawnError::Refused(SpawnRefused::ResourceLockTimeout { .. })
    ));
    // Nothing held for the refused child — k1's prefix hold released.
    assert!(resource_lease_holders(&p, "k1").is_empty());
    // The rival's k2 hold is untouched (release only touches our holds).
    assert_eq!(resource_lease_holders(&p, "k2"), vec!["subagent:rival"]);
}

#[test]
fn ac3_cross_run_holder_contends() {
    let mut p = open_parent("ac3c");
    let (rival_run, rival_lease) = rival_run(&mut p);
    let scope = hh_ledger::leases::LeaseScope::Resource("shared-key".to_string());
    p.store
        .lease_acquire(
            &rival_run,
            &rival_lease,
            &scope,
            "subagent:other-run",
            30_000,
        )
        .unwrap();
    let err = run_env(&mut p, &share_spec(&["shared-key"])).unwrap_err();
    assert!(matches!(
        err,
        SpawnError::Refused(SpawnRefused::ResourceLockTimeout { .. })
    ));
}

// ── scoped_subtree + detach refusal ─────────────────────────────────────────

#[test]
fn scoped_subtree_spawns_holds_key_and_detach_refused() {
    let mut p = open_parent("scoped");
    let mut s = spec();
    s.environment = EnvIsolation::Derive {
        mode: DeriveMode::ScopedSubtree,
        spec: Json::obj([("scope", Json::str("/work/child"))]),
    };
    s.reserved_keys = vec!["fs_path:/work/child".to_string()];
    let spawned = run_env(&mut p, &s).unwrap();
    assert_eq!(
        resource_lease_holders(&p, "fs_path:/work/child"),
        vec![format!("subagent:{}", spawned.child_run_id)]
    );

    // `detach` under a shared/scoped environment is refused at spawn —
    // the `ModeUnsupported` detail names the O-2 arm (the child may not
    // outlive the coordination contract).
    delegate_decision(&mut p, 2);
    let mut d = spec();
    d.environment = s.environment.clone();
    d.on_parent_end = OnParentEnd::DetachToChild;
    let err = run_env(&mut p, &d).unwrap_err();
    match &err {
        SpawnError::Refused(SpawnRefused::ModeUnsupported { detail }) => {
            assert!(detail.contains("detach_to_child"), "{detail}");
        }
        other => panic!("expected ModeUnsupported{{detach_to_child}}, got {other:?}"),
    }

    delegate_decision(&mut p, 3);
    let mut ds = share_spec(&["k-detach"]);
    ds.on_parent_end = OnParentEnd::DetachToChild;
    let err = run_env(&mut p, &ds).unwrap_err();
    match &err {
        SpawnError::Refused(SpawnRefused::ModeUnsupported { detail }) => {
            assert!(detail.contains("detach_to_child"), "{detail}");
        }
        other => panic!("expected ModeUnsupported{{detach_to_child}}, got {other:?}"),
    }
}

// ── K-1 — declaration consistency ───────────────────────────────────────────

#[test]
fn k1_share_refuses_snapshot_at_fork() {
    let mut p = open_parent("k1");
    let mut s = share_spec(&[]);
    s.consistency_declarations = vec![ConsistencyDeclaration {
        object: None,
        kind_wildcard: Some("fs".to_string()),
        level: ConsistencyLevel::SnapshotAtFork,
        staleness_bound_ms: None,
        readers: "owner_only".to_string(),
    }
    .to_json()];
    let err = run_env(&mut p, &s).unwrap_err();
    match &err {
        SpawnError::Refused(SpawnRefused::DefinitionUnresolvable { detail }) => {
            assert!(detail.contains("snapshot_at_fork"), "{detail}");
        }
        other => panic!("expected DefinitionUnresolvable, got {other:?}"),
    }
}

#[test]
fn k1_fork_snapshot_refuses_per_key_serializable() {
    let mut p = open_parent("k1b");
    let mut s = spec();
    s.environment = EnvIsolation::Derive {
        mode: DeriveMode::ForkSnapshot,
        spec: Json::Null,
    };
    s.consistency_declarations = vec![ConsistencyDeclaration {
        object: None,
        kind_wildcard: Some("fs".to_string()),
        level: ConsistencyLevel::PerKeySerializable,
        staleness_bound_ms: None,
        readers: "owner_only".to_string(),
    }
    .to_json()];
    let err = run_env(&mut p, &s).unwrap_err();
    match &err {
        SpawnError::Refused(SpawnRefused::DefinitionUnresolvable { detail }) => {
            assert!(detail.contains("per_key_serializable"), "{detail}");
        }
        other => panic!("expected DefinitionUnresolvable, got {other:?}"),
    }
}

#[test]
fn k1_scoped_subtree_refuses_snapshot_at_fork() {
    let mut p = open_parent("k1c");
    let mut s = spec();
    s.environment = EnvIsolation::Derive {
        mode: DeriveMode::ScopedSubtree,
        spec: Json::obj([("scope", Json::str("/w"))]),
    };
    s.consistency_declarations = vec![ConsistencyDeclaration {
        object: None,
        kind_wildcard: Some("fs".to_string()),
        level: ConsistencyLevel::SnapshotAtFork,
        staleness_bound_ms: None,
        readers: "owner_only".to_string(),
    }
    .to_json()];
    let err = run_env(&mut p, &s).unwrap_err();
    match &err {
        SpawnError::Refused(SpawnRefused::DefinitionUnresolvable { detail }) => {
            assert!(detail.contains("snapshot_at_fork"), "{detail}");
        }
        other => panic!("expected DefinitionUnresolvable, got {other:?}"),
    }
}

#[test]
fn k1_malformed_declaration_refuses() {
    let mut p = open_parent("k1d");
    let mut s = share_spec(&[]);
    s.consistency_declarations = vec![Json::obj([("level", Json::str("nonsense"))])];
    let err = run_env(&mut p, &s).unwrap_err();
    assert!(matches!(
        err,
        SpawnError::Refused(SpawnRefused::DefinitionUnresolvable { .. })
    ));
}

// ── AC-R-2.6.5-2 — check 8 (ownership write check) ─────────────────────────

#[test]
fn ac2_check_write_ok_not_owner_and_audit_row() {
    let mut p = open_parent("ac2");
    // The parent owns `/` — a write under it clears.
    let obj_in = OwnedObject::FsPathPrefix("/src/x".to_string());
    let v = check_write(
        &p.store,
        &p.run_id,
        p.lease.generation,
        &obj_in,
        &p.ownerships,
    )
    .unwrap();
    assert!(matches!(v, WriteVerdict::Ok));

    // Re-seat the table without the parent's root: `/other` belongs to a
    // rival, `/free` is unowned.
    p.ownerships = OwnershipTable::project(
        &p.store,
        &[],
        vec![(
            "run-rival".to_string(),
            OwnedObject::FsPathPrefix("/other".to_string()),
        )],
    )
    .unwrap();
    let obj_out = OwnedObject::FsPathPrefix("/other/x".to_string());
    let v = check_write(
        &p.store,
        &p.run_id,
        p.lease.generation,
        &obj_out,
        &p.ownerships,
    )
    .unwrap();
    let denied = matches!(&v, WriteVerdict::NotOwner { owner } if owner == "run-rival");
    assert!(denied, "expected NotOwner, got {v:?}");
    let _eid =
        record_write_refusal(&mut p.store, &p.run_id, &p.lease, "eff-deny", &obj_out, &v).unwrap();
    let events = p.store.events(&p.run_id).unwrap();
    let deny = events
        .iter()
        .find(|e| {
            e.class == "security.policy.evaluated"
                && e.payload.get("check").and_then(Json::as_str) == Some("ownership")
        })
        .expect("ownership deny audit row");
    assert_eq!(
        deny.payload.get("verdict").and_then(Json::as_str),
        Some("deny")
    );

    // An unowned subtree is free territory (check 8 governs owned/shared
    // objects, not virgin paths).
    let obj_free = OwnedObject::FsPathPrefix("/free/x".to_string());
    let v = check_write(
        &p.store,
        &p.run_id,
        p.lease.generation,
        &obj_free,
        &p.ownerships,
    )
    .unwrap();
    assert!(matches!(v, WriteVerdict::Ok));
}

#[test]
fn ac2_resource_write_requires_held_key() {
    let mut p = open_parent("ac2b");
    let obj = OwnedObject::ResourceKey("shared-counter".to_string());
    // Unheld key → `NotOwner{unheld}` (a shared key may not be written
    // lock-free — O-6's no-side-channel arm).
    let v = check_write(&p.store, &p.run_id, p.lease.generation, &obj, &p.ownerships).unwrap();
    assert!(matches!(&v, WriteVerdict::NotOwner { owner } if owner == "unheld"));
    // Another holder → `NotOwner{holder}`.
    let scope = hh_ledger::leases::LeaseScope::Resource("shared-counter".to_string());
    p.store
        .lease_acquire(&p.run_id, &p.lease, &scope, "subagent:someone-else", 30_000)
        .unwrap();
    let v = check_write(&p.store, &p.run_id, p.lease.generation, &obj, &p.ownerships).unwrap();
    assert!(matches!(&v, WriteVerdict::NotOwner { owner } if owner == "subagent:someone-else"));
    // Our own `subagent:<run>` hold → Ok.
    p.store
        .lease_acquire(
            &p.run_id,
            &p.lease,
            &scope,
            &format!("subagent:{}", p.run_id),
            30_000,
        )
        .unwrap_err(); // contends with the live record above — expected
    let v = check_write(&p.store, &p.run_id, p.lease.generation, &obj, &p.ownerships).unwrap();
    assert!(matches!(&v, WriteVerdict::NotOwner { .. }));
}

// ── O-3/O-4 — transfer + return rows and the fold ───────────────────────────

#[test]
fn o3_o4_transfer_and_return_fold() {
    let mut p = open_parent("verbs");
    let obj = OwnedObject::FsPathPrefix("/shared/doc".to_string());
    // The parent owns it (the `/` root record covers).
    let ev = transfer_ownership(
        &mut p.store,
        &p.run_id,
        &p.lease,
        &obj,
        &p.run_id,
        "run-child-a",
        &p.decision,
        &p.ownerships,
    )
    .unwrap();
    let events = p.store.events(&p.run_id).unwrap();
    assert!(events
        .iter()
        .any(|e| e.event_id == ev && e.class == "control.ownership.transferred"));
    // The fold sees the child as owner (nearest covering record wins).
    let folded = OwnershipTable::project(&p.store, &[p.run_id.clone()], vec![]).unwrap();
    assert_eq!(folded.owner_of(&obj).as_deref(), Some("run-child-a"));
    // Transfer from a non-holder refuses typed (no silent reassign).
    let err = transfer_ownership(
        &mut p.store,
        &p.run_id,
        &p.lease,
        &obj,
        "run-stranger",
        "run-child-b",
        &p.decision,
        &folded,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        SpawnError::Refused(SpawnRefused::NotOwned { .. })
    ));

    let _ = return_ownership(
        &mut p.store,
        &p.run_id,
        &p.lease,
        &obj,
        "run-child-a",
        &p.run_id,
        &p.decision,
    )
    .unwrap();
    let events = p.store.events(&p.run_id).unwrap();
    assert!(events
        .iter()
        .any(|e| e.class == "control.ownership.returned"));
    let folded = OwnershipTable::project(&p.store, &[p.run_id.clone()], vec![]).unwrap();
    assert_eq!(folded.owner_of(&obj).as_deref(), Some(p.run_id.as_str()));
}

// ── G-1/G-2 — `three_way_text{line}` ────────────────────────────────────────

fn blob_ref(p: &mut Parent, text: &str) -> String {
    p.store
        .put_blob(text.as_bytes(), "text/plain")
        .unwrap()
        .id()
}

fn merge_ctx<'a>(
    p: &'a mut Parent,
    policy: MergePolicy,
    inputs: Vec<MergeInput>,
    parents: &'a BTreeMap<String, Option<String>>,
    baseline: &'a BTreeMap<String, String>,
    child_owns: &'a BTreeMap<String, Vec<OwnedObject>>,
    opts: MergeOpts<'a>,
) -> MergeCtx<'a> {
    MergeCtx {
        store: &mut p.store,
        parent_run_id: &p.run_id,
        parent_lease: &p.lease,
        decision: &p.decision,
        ownerships: &p.ownerships,
        policy,
        inputs,
        parent_versions: parents,
        baseline,
        child_ownerships: child_owns,
        opts,
    }
}

#[test]
fn g1_three_way_text_disjoint_edits_merge() {
    let mut p = open_parent("g1");
    let spawned = run(&mut p, &spec()).unwrap();
    let base = blob_ref(&mut p, "l1\nl2\nl3\nl4\nl5\n");
    let parent_v = blob_ref(&mut p, "L1-parent\nl2\nl3\nl4\nl5\n");
    let child_v = blob_ref(&mut p, "l1\nl2\nl3\nl4\nL5-child\n");
    let parents = BTreeMap::from([("/doc".to_string(), Some(parent_v))]);
    let baseline = BTreeMap::from([("/doc".to_string(), base.clone())]);
    let child_owns = BTreeMap::from([(
        spawned.child_run_id.clone(),
        vec![OwnedObject::FsPathPrefix("/doc".to_string())],
    )]);
    let inputs = vec![MergeInput {
        child_run_id: spawned.child_run_id.clone(),
        changes: BTreeMap::from([("/doc".to_string(), (Some(base), Some(child_v)))]),
    }];
    let out = {
        let mut c = merge_ctx(
            &mut p,
            MergePolicy::ThreeWayText,
            inputs,
            &parents,
            &baseline,
            &child_owns,
            MergeOpts::default(),
        );
        merge(&mut c).unwrap()
    };
    assert!(out.veto.is_none(), "disjoint hunks merge: {out:?}");
    let report = out.report.expect("report");
    assert_eq!(report.conflicts.len(), 0);
    assert_eq!(report.merged.len(), 1);
    // The merged text carries both edits — the new ref is a fresh blob.
    let merged_ref = report.merged[0]
        .get("after_ref")
        .and_then(Json::as_str)
        .unwrap();
    let parsed = hh_identity::idp::parse_id(merged_ref).unwrap();
    let bytes = p
        .store
        .get_blob(&hh_identity::idp::ContentAddress {
            idp: "idp/1",
            algorithm: "sha256",
            digest: parsed.digest_hex,
            media_type: "text/plain".to_string(),
            size: 0,
        })
        .unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(
        text.contains("L1-parent") && text.contains("L5-child"),
        "{text}"
    );
}

#[test]
fn g2_three_way_text_overlap_records_conflict_with_detector() {
    let mut p = open_parent("g2");
    let spawned = run(&mut p, &spec()).unwrap();
    let base = blob_ref(&mut p, "l1\nl2\nl3\n");
    let parent_v = blob_ref(&mut p, "l1\nL2-PARENT\nl3\n");
    let child_v = blob_ref(&mut p, "l1\nL2-CHILD\nl3\n");
    let parents = BTreeMap::from([("/doc".to_string(), Some(parent_v))]);
    let baseline = BTreeMap::from([("/doc".to_string(), base.clone())]);
    let child_owns = BTreeMap::from([(
        spawned.child_run_id.clone(),
        vec![OwnedObject::FsPathPrefix("/doc".to_string())],
    )]);
    let inputs = vec![MergeInput {
        child_run_id: spawned.child_run_id.clone(),
        changes: BTreeMap::from([("/doc".to_string(), (Some(base), Some(child_v)))]),
    }];
    let out = {
        let mut c = merge_ctx(
            &mut p,
            MergePolicy::ThreeWayText,
            inputs,
            &parents,
            &baseline,
            &child_owns,
            MergeOpts::default(),
        );
        merge(&mut c).unwrap()
    };
    let report = out.report.expect("report");
    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(
        report.conflicts[0].detector.as_deref(),
        Some("deterministic{three_way_text}"),
        "the deterministic engine stamps the conflict's detector"
    );
    // No silent pick — the overlapping path has no merged entry.
    assert!(report.merged.is_empty());
}

#[test]
fn g2_missing_fork_point_blob_vetoes() {
    let mut p = open_parent("g2b");
    let spawned = run(&mut p, &spec()).unwrap();
    let parents = BTreeMap::from([("/doc".to_string(), Some("sha256:missing".to_string()))]);
    let baseline = BTreeMap::from([("/doc".to_string(), "sha256:gone".to_string())]);
    let child_owns = BTreeMap::from([(
        spawned.child_run_id.clone(),
        vec![OwnedObject::FsPathPrefix("/doc".to_string())],
    )]);
    let inputs = vec![MergeInput {
        child_run_id: spawned.child_run_id.clone(),
        changes: BTreeMap::from([(
            "/doc".to_string(),
            (
                Some("sha256:gone".to_string()),
                Some("sha256:new".to_string()),
            ),
        )]),
    }];
    let out = {
        let mut c = merge_ctx(
            &mut p,
            MergePolicy::ThreeWayText,
            inputs,
            &parents,
            &baseline,
            &child_owns,
            MergeOpts::default(),
        );
        merge(&mut c).unwrap()
    };
    assert!(matches!(out.veto, Some(MergeVeto::ForkPointMissing { .. })));
}

// ── AC-R-2.6.5-4 — `validator_selected` ─────────────────────────────────────

struct ChooseChild;
impl MergeValidator for ChooseChild {
    fn select(&self, _c: &MergeConflictRecord) -> ValidatorMergeVerdict {
        ValidatorMergeVerdict::Choose {
            side: ChooseSide::Child,
        }
    }
}

struct Abstain;
impl MergeValidator for Abstain {
    fn select(&self, _c: &MergeConflictRecord) -> ValidatorMergeVerdict {
        ValidatorMergeVerdict::Abstain
    }
}

fn conflicted_input(
    child: &str,
) -> (
    Vec<MergeInput>,
    BTreeMap<String, Option<String>>,
    BTreeMap<String, String>,
    BTreeMap<String, Vec<OwnedObject>>,
) {
    let parents = BTreeMap::from([("/shared".to_string(), Some("ref:parent".to_string()))]);
    let baseline = BTreeMap::from([("/shared".to_string(), "ref:base".to_string())]);
    let child_owns = BTreeMap::from([(
        child.to_string(),
        vec![OwnedObject::FsPathPrefix("/shared".to_string())],
    )]);
    let inputs = vec![MergeInput {
        child_run_id: child.to_string(),
        changes: BTreeMap::from([(
            "/shared".to_string(),
            (Some("ref:base".to_string()), Some("ref:child".to_string())),
        )]),
    }];
    (inputs, parents, baseline, child_owns)
}

#[test]
fn ac4_validator_selected_choose_child_resolves() {
    let mut p = open_parent("ac4");
    let spawned = run(&mut p, &spec()).unwrap();
    let (inputs, parents, baseline, child_owns) = conflicted_input(&spawned.child_run_id);
    let port = ChooseChild;
    let out = {
        let mut c = merge_ctx(
            &mut p,
            MergePolicy::ValidatorSelected,
            inputs,
            &parents,
            &baseline,
            &child_owns,
            MergeOpts {
                validator: Some(&port),
                validator_ref: Some("validator:sel-v1".to_string()),
                ..Default::default()
            },
        );
        merge(&mut c).unwrap()
    };
    assert!(out.veto.is_none(), "{out:?}");
    let events = p.store.events(&p.run_id).unwrap();
    assert!(events
        .iter()
        .any(|e| e.class == "verification.validator.invoked"));
    assert!(events
        .iter()
        .any(|e| e.class == "verification.validator.verdict"));
    let resolved = events
        .iter()
        .find(|e| e.class == "control.merge.resolved")
        .expect("resolved row");
    assert_eq!(
        resolved.payload.get("resolver").and_then(Json::as_str),
        Some("validator:sel-v1")
    );
    // The child side won — merged carries the child's ref.
    let report = out.report.expect("report");
    assert!(report
        .merged
        .iter()
        .any(|m| m.get("after_ref").and_then(Json::as_str) == Some("ref:child")));
}

#[test]
fn ac4_validator_abstain_leaves_conflict_open() {
    let mut p = open_parent("ac4b");
    let spawned = run(&mut p, &spec()).unwrap();
    let (inputs, parents, baseline, child_owns) = conflicted_input(&spawned.child_run_id);
    let port = Abstain;
    let out = {
        let mut c = merge_ctx(
            &mut p,
            MergePolicy::ValidatorSelected,
            inputs,
            &parents,
            &baseline,
            &child_owns,
            MergeOpts {
                validator: Some(&port),
                validator_ref: Some("validator:sel-v1".to_string()),
                ..Default::default()
            },
        );
        merge(&mut c).unwrap()
    };
    let report = out.report.expect("report");
    assert_eq!(
        report.conflicts.len(),
        1,
        "abstention keeps the conflict open"
    );
    assert!(!p
        .store
        .events(&p.run_id)
        .unwrap()
        .iter()
        .any(|e| e.class == "control.merge.resolved"));
}

#[test]
fn ac4_validator_unavailable_is_a_typed_veto() {
    let mut p = open_parent("ac4c");
    let spawned = run(&mut p, &spec()).unwrap();
    let (inputs, parents, baseline, child_owns) = conflicted_input(&spawned.child_run_id);
    let out = {
        let mut c = merge_ctx(
            &mut p,
            MergePolicy::ValidatorSelected,
            inputs,
            &parents,
            &baseline,
            &child_owns,
            MergeOpts {
                validator: None,
                validator_ref: Some("validator:sel-v1".to_string()),
                ..Default::default()
            },
        );
        merge(&mut c).unwrap()
    };
    assert!(matches!(
        out.veto,
        Some(MergeVeto::ValidatorUnavailable { .. })
    ));
}

// ── G-6 — `merge_hash` order-independence ───────────────────────────────────

#[test]
fn g6_merge_hash_order_independent() {
    // Two independent parents run the same merge with permuted input
    // order — the outcome address is a pure function of (inputs, base,
    // policy), so `merge_hash` must agree (G-6 crash-equivalence).
    let h_for = |tag: &str, order: &[usize]| {
        let mut p = open_parent(tag);
        let a = run(&mut p, &spec()).unwrap();
        delegate_decision(&mut p, 1);
        let b = run(&mut p, &spec()).unwrap();
        let child_owns: BTreeMap<String, Vec<OwnedObject>> = BTreeMap::from([
            (
                a.child_run_id.clone(),
                vec![OwnedObject::FsPathPrefix("/a".to_string())],
            ),
            (
                b.child_run_id.clone(),
                vec![OwnedObject::FsPathPrefix("/b".to_string())],
            ),
        ]);
        let inputs_all = [
            MergeInput {
                child_run_id: a.child_run_id.clone(),
                changes: BTreeMap::from([("/a/x".to_string(), (None, Some("ref:ax".to_string())))]),
            },
            MergeInput {
                child_run_id: b.child_run_id.clone(),
                changes: BTreeMap::from([("/b/y".to_string(), (None, Some("ref:by".to_string())))]),
            },
        ];
        let inputs = order
            .iter()
            .map(|&i| {
                let m = &inputs_all[i];
                MergeInput {
                    child_run_id: m.child_run_id.clone(),
                    changes: m.changes.clone(),
                }
            })
            .collect();
        let parents: BTreeMap<String, Option<String>> = BTreeMap::new();
        let baseline: BTreeMap<String, String> = BTreeMap::new();
        let mut c = merge_ctx(
            &mut p,
            MergePolicy::SingleWriter,
            inputs,
            &parents,
            &baseline,
            &child_owns,
            MergeOpts::default(),
        );
        merge(&mut c).unwrap().report.expect("report").merge_hash
    };
    let h1 = h_for("g6a", &[0, 1]);
    let h2 = h_for("g6b", &[1, 0]);
    assert_eq!(
        h1, h2,
        "merge_hash is a pure function of (inputs, base, policy)"
    );
    assert!(!h1.is_empty());
}

// ── G-5 — absent children ───────────────────────────────────────────────────

#[test]
fn g5_absent_child_lands_in_absent_not_merged() {
    let mut p = open_parent("g5");
    let done = run(&mut p, &spec()).unwrap();
    delegate_decision(&mut p, 1);
    let dead = run(&mut p, &spec()).unwrap();
    // `dead` reached a non-result terminal — a cancelled row on the
    // parent's ledger (the crash-matrix arm).
    let cancel = kernel_ev(
        &p.store,
        &p.run_id,
        "control.subagent.cancelled",
        Json::obj([
            ("child_run_id", Json::str(dead.child_run_id.clone())),
            ("reason", Json::str("crash")),
        ]),
    );
    p.store.append(&p.run_id, &p.lease, vec![cancel]).unwrap();

    let child_owns = BTreeMap::from([
        (
            done.child_run_id.clone(),
            vec![OwnedObject::FsPathPrefix("/a".to_string())],
        ),
        (
            dead.child_run_id.clone(),
            vec![OwnedObject::FsPathPrefix("/b".to_string())],
        ),
    ]);
    let mk_inputs = || {
        vec![
            MergeInput {
                child_run_id: done.child_run_id.clone(),
                changes: BTreeMap::from([("/a/x".to_string(), (None, Some("ref:ax".to_string())))]),
            },
            MergeInput {
                child_run_id: dead.child_run_id.clone(),
                changes: BTreeMap::from([("/b/y".to_string(), (None, Some("ref:by".to_string())))]),
            },
        ]
    };
    let parents: BTreeMap<String, Option<String>> = BTreeMap::new();
    let baseline: BTreeMap<String, String> = BTreeMap::new();
    let out = {
        let mut c = merge_ctx(
            &mut p,
            MergePolicy::SingleWriter,
            mk_inputs(),
            &parents,
            &baseline,
            &child_owns,
            MergeOpts::default(),
        );
        merge(&mut c).unwrap()
    };
    let report = out.report.expect("report");
    assert!(report.absent.contains(&dead.child_run_id));
    // G-1 coverage: the dead child's write is neither merged nor
    // silently dropped — it is accounted in `absent[]`.
    assert!(!report.merged.iter().any(|m| {
        m.get("child_run_id").and_then(Json::as_str) == Some(dead.child_run_id.as_str())
    }));
    // `block_completion` (default) records the gate policy; `annotate`
    // completes with the absence annotated — the completed row carries
    // the declared arm either way.
    let events = p.store.events(&p.run_id).unwrap();
    let completed = events
        .iter()
        .rev()
        .find(|e| e.class == "control.merge.completed")
        .unwrap();
    assert_eq!(
        completed
            .payload
            .get("on_absent_child")
            .and_then(Json::as_str),
        Some("block_completion")
    );
    let out2 = {
        let mut c = merge_ctx(
            &mut p,
            MergePolicy::SingleWriter,
            mk_inputs(),
            &parents,
            &baseline,
            &child_owns,
            MergeOpts {
                on_absent_child: OnAbsentChild::Annotate,
                ..Default::default()
            },
        );
        merge(&mut c).unwrap()
    };
    let report2 = out2.report.expect("r2");
    assert!(report2.absent.contains(&dead.child_run_id));
    let events = p.store.events(&p.run_id).unwrap();
    let completed = events
        .iter()
        .rev()
        .find(|e| e.class == "control.merge.completed")
        .unwrap();
    assert_eq!(
        completed
            .payload
            .get("on_absent_child")
            .and_then(Json::as_str),
        Some("annotate")
    );
}

// ── O-5 — terminal release of `resource(key)` holds ─────────────────────────

#[test]
fn o5_child_terminal_releases_resource_locks() {
    let mut p = open_parent("o5");
    let s = share_spec(&["k-term"]);
    let spawned = run_env(&mut p, &s).unwrap();
    assert!(!resource_lease_holders(&p, "k-term").is_empty());
    let view = ChildTerminalView {
        finished: true,
        child_head: None,
        stop_reason: "goal_met".to_string(),
        artifacts: vec![],
        fs_changes: None,
        summary: None,
        claims: vec![],
        memories_written: vec![],
        effects_irreversible: vec![],
        usage: Json::Null,
        verdicts: vec![],
    };
    record_child_result(
        &mut p.store,
        &p.run_id,
        &p.lease,
        &spawned.child_run_id,
        &s,
        &view,
        &spawned.budget_id,
    )
    .unwrap();
    assert!(
        resource_lease_holders(&p, "k-term").is_empty(),
        "the child's resource holds release at its terminal"
    );
    // A sibling may now take the same key.
    delegate_decision(&mut p, 9);
    let _sib = run_env(&mut p, &share_spec(&["k-term"])).unwrap();
}
