//! S4.14b acceptance — the label-branch slice of R-2.8.2 (AC-R-2.8.2-6;
//! §5g.2 `spawn_branch`/`return_from_branch`; ADR-0054 D1/D4):
//!
//! - `spawn_branch` lands `control.subagent.spawned{isolation =
//!   label_branch, ctx0, clearance}` — the parent's `ctx` never mutates.
//! - `labeled_return` re-enters at `external` with union taint
//!   (`security.label.applied`), the claim checked against the floor —
//!   a dropped tag is `BranchTaintEscaped`.
//! - `attested_return` over a capacity-bounded schema re-enters at
//!   `environment`, `taint = ∅` (`security.label.endorsed{capacity_bits}`);
//!   a free-string schema is `BasisNotAllowed`.
//! - `spawn_branch` with `ceiling > clearance` is `ClearanceExceeded`;
//!   a `branch` remedy consumed mints the same `spawned` row.
//! - A child-side (delegate-class) endorsement of the parent's effect is
//!   `IllegitimateEndorsement` — approvals never transfer.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hh_budget::{Account, BudgetMode, BudgetScope, BudgetScopeKind, BudgetSpec};
use hh_env::handle::OnParentEnd;
use hh_hir::kinds::{EffectClass, EffectDomain};
use hh_hir::leaves::Text;
use hh_hir::records::{GoalOrigin, GoalRecord, Grant, GrantConstraints};
use hh_hir::refs::Ref;
use hh_ledger::manifest::{EventRef, RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_monitor::decision::DecisionScope;
use hh_monitor::delegate::LiveCoords;
use hh_monitor::handle::{AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis};
use hh_monitor::table::HandleTable;
use hh_ontology::control::Owner;
use hh_ontology::dimensions::{DimensionId, DimensionKey};
use hh_provenance::endorse::{endorse, ContentKind, EndorsementBasis, EndorsementError};
use hh_provenance::{
    AuthorityClass, Label, Origin, PersistenceScope, ProvenanceRecord, ReaderSet, Remedy, TaintTag,
};
use hh_subagent::branch::{
    consume_branch_remedy, return_from_branch, spawn_branch, BranchError, BranchExit,
};
use hh_subagent::ownership::OwnershipTable;
use hh_subagent::spawn::SpawnCtx;
use hh_subagent::types::*;
use hh_wire::json::Json;

fn dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!(
        "hh-subagent-s414b-{tag}-{}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&p);
    p
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
    let manifest = RunManifest::minimal(RunKind::Agent);
    let (run_id, lease) = store.open_run(manifest, "writer-parent").unwrap();
    let mut p = Parent {
        store,
        run_id: run_id.clone(),
        lease,
        budget_id: String::new(),
        decision: EventRef {
            run_id: run_id.clone(),
            event_id: String::new(),
        },
        handle_id: HandleId("hnd-parent".to_string()),
        handles: HandleTable::default(),
        ownerships: OwnershipTable::default(),
        tools: Vec::new(),
    };
    // The delegate decision the spawn hangs from.
    let ev = hh_ledger::event::Event {
        event_id: p.store.alloc_id("evt"),
        class: "control.decision".to_string(),
        ts: p.store.ts_now(),
        hlc: None,
        producer: hh_ledger::event::Producer::kernel("kernel:test"),
        scope: hh_ledger::event::Scope::default(),
        parent_event_id: p.store.head_event_id(&p.run_id).unwrap(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: Default::default(),
        provenance: Some(ProvenanceRecord::kernel("kernel:test", p.store.now_ms())),
        content_kind: None,
        payload: Json::obj([
            ("kind", Json::str("delegate")),
            ("delegation_reason", Json::str("isolation")),
            ("decision_id", Json::str("dec-0")),
        ]),
    };
    p.decision = EventRef {
        run_id: run_id.clone(),
        event_id: ev.event_id.clone(),
    };
    p.store.append(&p.run_id, &p.lease, vec![ev]).unwrap();
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
        permission_ref: hh_compiler::plan::PinnedRef {
            semantic_id: "perm:root".to_string(),
            version_id: "v1".to_string(),
        },
        holder: Ref::pinned("agent_process", p.run_id.clone()),
        issuer: ProvenanceRecord::kernel("kernel:test", 0),
        grants: vec![
            grant(EffectDomain::FsRead, "*", true),
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

fn spec() -> SubagentSpec {
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
        wait_mode: WaitMode::Background,
        wait_timeout_ms: 60_000,
        on_parent_end: OnParentEnd::Teardown,
        delegation_reason: DelegationReason::Isolation,
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

/// A tainted `external` subject — e.g. the branch's labeled summary.
fn tainted_subject(tags: &[&str]) -> ProvenanceRecord {
    ProvenanceRecord {
        origin: Origin::Tool {
            capability: "cap:branch.out".to_string(),
            invocation_ref: "inv-1".to_string(),
            inner_source: None,
        },
        authority: AuthorityClass::External,
        taint: tags
            .iter()
            .map(|t| TaintTag::Tool {
                capability: t.to_string(),
                inner_source: None,
            })
            .collect(),
        readers: ReaderSet::Public,
        scope: PersistenceScope::Run,
        derived_from: vec![],
        created_at: 1,
        attestation: None,
    }
}

/// Commit the branch's returned record as a parent-run event (the
/// `subject_ref` `check_endorsement` resolves at append).
fn commit_subject(p: &mut Parent, subject: &ProvenanceRecord) -> String {
    let ev = hh_ledger::event::Event {
        event_id: p.store.alloc_id("evt"),
        class: "context.observation.recorded".to_string(),
        ts: p.store.ts_now(),
        hlc: None,
        producer: hh_ledger::event::Producer::kernel("kernel:test"),
        scope: hh_ledger::event::Scope::default(),
        parent_event_id: p.store.head_event_id(&p.run_id).unwrap(),
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: Default::default(),
        provenance: Some(subject.clone()),
        content_kind: Some(hh_provenance::ContentKind::ClosedSchemaValue),
        payload: Json::obj([("admission", Json::str("as_declared"))]),
    };
    let id = ev.event_id.clone();
    p.store.append(&p.run_id, &p.lease, vec![ev]).unwrap();
    id
}

fn events_of(p: &Parent, class: &str) -> Vec<Json> {
    p.store
        .events(&p.run_id)
        .unwrap()
        .iter()
        .filter(|e| e.class == class)
        .map(|e| e.payload.clone())
        .collect()
}

#[test]
fn spawn_branch_lands_spawned_with_ctx0_and_clearance() {
    let mut p = open_parent("spawn");
    let ctx0 = Label::at(AuthorityClass::Delegate);
    let spawned = spawn_branch(&mut ctx(&mut p), &spec(), &ctx0, AuthorityClass::Delegate).unwrap();
    let rows = events_of(&p, "control.subagent.spawned");
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(
        row.get("isolation").and_then(Json::as_str),
        Some("label_branch")
    );
    assert!(row.get("ctx0").is_some(), "ctx0 member missing: {row:?}");
    assert_eq!(
        row.get("clearance").and_then(Json::as_str),
        Some("delegate")
    );
    // The child's run opened — `isolation` round-trips through the spec.
    assert!(p.store.run_ids().contains(&spawned.child_run_id));
}

#[test]
fn spawn_branch_ceiling_above_clearance_is_clearance_exceeded() {
    let mut p = open_parent("clearance");
    let mut s = spec();
    s.ceiling = Some(AuthorityClass::Principal);
    match spawn_branch(
        &mut ctx(&mut p),
        &s,
        &Label::at(AuthorityClass::Delegate),
        AuthorityClass::Delegate,
    ) {
        Err(SpawnError::Refused(SpawnRefused::ClearanceExceeded {
            requested,
            clearance,
        })) => {
            assert_eq!(requested, AuthorityClass::Principal);
            assert_eq!(clearance, AuthorityClass::Delegate);
        }
        other => panic!("expected ClearanceExceeded, got {other:?}"),
    }
}

#[test]
fn labeled_return_reenters_at_external_with_union_taint() {
    let mut p = open_parent("return");
    let mut ctx0 = Label::at(AuthorityClass::Delegate);
    ctx0.taint.insert(TaintTag::Tool {
        capability: "cap:web".to_string(),
        inner_source: None,
    });
    let spawned = spawn_branch(&mut ctx(&mut p), &spec(), &ctx0, AuthorityClass::Delegate).unwrap();
    let subject = tainted_subject(&["cap:email"]);
    return_from_branch(
        &mut p.store,
        &p.run_id,
        &p.lease,
        &spawned.child_run_id,
        &ctx0,
        &BranchExit::LabeledReturn {
            subject: subject.clone(),
            subject_ref: "item:summary".to_string(),
            declared: None,
        },
    )
    .unwrap();
    // `security.label.applied` — the re-entry is `external` with the
    // union taint (ctx0's `cap:web` ∪ subject's `cap:email`).
    let applied = events_of(&p, "security.label.applied");
    assert_eq!(applied.len(), 1);
    let label = hh_provenance::label_from_json(applied[0].get("label").expect("label member"))
        .expect("label decodes");
    assert_eq!(label.authority, AuthorityClass::External);
    let taints: BTreeSet<String> = label.taint.iter().map(|t| t.as_string()).collect();
    assert!(
        taints.contains("tool:cap:web"),
        "ctx0 taint union: {taints:?}"
    );
    assert!(
        taints.contains("tool:cap:email"),
        "subject taint union: {taints:?}"
    );
    // The exit record lands in the same append.
    let exits = events_of(&p, "control.subagent.branch_exited");
    assert_eq!(
        exits[0].get("exit").and_then(Json::as_str),
        Some("labeled_return")
    );
}

#[test]
fn labeled_return_declared_label_dropping_taint_is_branch_taint_escaped() {
    let mut p = open_parent("escape");
    let mut ctx0 = Label::at(AuthorityClass::Delegate);
    ctx0.taint.insert(TaintTag::Tool {
        capability: "cap:web".to_string(),
        inner_source: None,
    });
    let spawned = spawn_branch(&mut ctx(&mut p), &spec(), &ctx0, AuthorityClass::Delegate).unwrap();
    // The child claims the return is clean — the declared label drops
    // ctx0's required taint.
    match return_from_branch(
        &mut p.store,
        &p.run_id,
        &p.lease,
        &spawned.child_run_id,
        &ctx0,
        &BranchExit::LabeledReturn {
            subject: tainted_subject(&["cap:email"]),
            subject_ref: "item:summary".to_string(),
            declared: Some(Label::at(AuthorityClass::External)),
        },
    ) {
        Err(BranchError::BranchTaintEscaped { detail }) => {
            assert!(detail.contains("cap:"), "{detail}");
        }
        other => panic!("expected BranchTaintEscaped, got {other:?}"),
    }
    // Nothing re-entered.
    assert!(events_of(&p, "security.label.applied").is_empty());
}

#[test]
fn attested_return_endorses_bounded_schema_and_refuses_free_text() {
    let mut p = open_parent("attested");
    let ctx0 = Label::at(AuthorityClass::Delegate);
    let spawned = spawn_branch(&mut ctx(&mut p), &spec(), &ctx0, AuthorityClass::Delegate).unwrap();
    let validator = ProvenanceRecord::kernel("kernel:validator", 0);
    // (b) A `boolean` schema — capacity_bits = 1 → `environment`, taint ∅.
    let subject = tainted_subject(&["cap:email"]);
    let subject_ref = commit_subject(&mut p, &subject);
    return_from_branch(
        &mut p.store,
        &p.run_id,
        &p.lease,
        &spawned.child_run_id,
        &ctx0,
        &BranchExit::AttestedReturn {
            subject: subject.clone(),
            subject_ref: subject_ref.clone(),
            schema: Json::obj([("type", Json::str("boolean"))]),
            validator: validator.clone(),
            validator_ref: "validator:shape".to_string(),
            cap_max: 8,
        },
    )
    .unwrap();
    let endorsed = events_of(&p, "security.label.endorsed");
    assert_eq!(endorsed.len(), 1);
    assert_eq!(
        endorsed[0].get("capacity_bits").and_then(Json::as_int),
        Some(1)
    );
    assert_eq!(
        endorsed[0]
            .get("to")
            .and_then(|j| j.get("authority"))
            .and_then(Json::as_str),
        Some("environment")
    );
    // (c) A free-string schema is BasisNotAllowed.
    match return_from_branch(
        &mut p.store,
        &p.run_id,
        &p.lease,
        &spawned.child_run_id,
        &ctx0,
        &BranchExit::AttestedReturn {
            subject: subject.clone(),
            subject_ref,
            schema: Json::obj([("type", Json::str("string"))]),
            validator,
            validator_ref: "validator:shape".to_string(),
            cap_max: 8,
        },
    ) {
        Err(BranchError::Endorsement(EndorsementError::BasisNotAllowed { .. })) => {}
        other => panic!("expected BasisNotAllowed, got {other:?}"),
    }
}

#[test]
fn abandon_records_the_exit_without_reentry() {
    let mut p = open_parent("abandon");
    let ctx0 = Label::at(AuthorityClass::Delegate);
    let spawned = spawn_branch(&mut ctx(&mut p), &spec(), &ctx0, AuthorityClass::Delegate).unwrap();
    return_from_branch(
        &mut p.store,
        &p.run_id,
        &p.lease,
        &spawned.child_run_id,
        &ctx0,
        &BranchExit::Abandon,
    )
    .unwrap();
    let exits = events_of(&p, "control.subagent.branch_exited");
    assert_eq!(exits.len(), 1);
    assert_eq!(exits[0].get("exit").and_then(Json::as_str), Some("abandon"));
    assert!(events_of(&p, "security.label.applied").is_empty());
    assert!(events_of(&p, "security.label.endorsed").is_empty());
}

#[test]
fn return_from_a_non_branch_child_is_refused() {
    let mut p = open_parent("nonbranch");
    // An ordinary `fresh` spawn — `isolation ≠ label_branch`.
    let spawned = hh_subagent::spawn::spawn(&mut ctx(&mut p), &spec()).unwrap();
    match return_from_branch(
        &mut p.store,
        &p.run_id,
        &p.lease,
        &spawned.child_run_id,
        &Label::at(AuthorityClass::Delegate),
        &BranchExit::Abandon,
    ) {
        Err(BranchError::NotALabelBranch { .. }) => {}
        other => panic!("expected NotALabelBranch, got {other:?}"),
    }
}

#[test]
fn branch_remedy_consumption_spawns_a_label_branch() {
    let mut p = open_parent("remedy");
    let remedy = Remedy::Branch {
        spec: spec().to_json().to_canonical_string(),
    };
    let spawned = consume_branch_remedy(
        &mut ctx(&mut p),
        &remedy,
        &Label::at(AuthorityClass::Delegate),
        AuthorityClass::Delegate,
    )
    .unwrap();
    let rows = events_of(&p, "control.subagent.spawned");
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("isolation").and_then(Json::as_str),
        Some("label_branch")
    );
    assert_eq!(
        rows[0].get("child_run_id").and_then(Json::as_str),
        Some(spawned.child_run_id.as_str())
    );
}

#[test]
fn child_side_approval_never_authorizes_the_parents_effect() {
    // The branch has no approval-transfer path; an endorsement minted by a
    // delegate-class endorser (the child's model origin) is
    // `IllegitimateEndorsement` at `endorse` — approvals never transfer.
    let mut subject = ProvenanceRecord::kernel("kernel:test", 0);
    subject.authority = AuthorityClass::Delegate;
    let child_endorser = ProvenanceRecord {
        origin: Origin::Model {
            model_ref: "model:child".to_string(),
            run_ref: "sub-1".to_string(),
            response_id: "r1".to_string(),
        },
        authority: AuthorityClass::Delegate,
        taint: BTreeSet::new(),
        readers: ReaderSet::Public,
        scope: PersistenceScope::Run,
        derived_from: vec![],
        created_at: 1,
        attestation: None,
    };
    let mut to = subject.label();
    to.authority = AuthorityClass::Principal;
    match endorse(
        &subject,
        "effect:parent.eff-9",
        &to,
        &child_endorser,
        EndorsementBasis::Approval,
        Some("perm:child-approval".to_string()),
        ContentKind::EffectIntent,
    ) {
        Err(EndorsementError::IllegitimateEndorsement { .. }) => {}
        other => panic!("expected IllegitimateEndorsement, got {other:?}"),
    }
}

// ── hosted children (§5e.3 T7; C-10; AC-R-2.8.1-4 / AC-R-2.1.5-6) ─────────

use hh_subagent::hosted::{HostedOpen, HostedPlane, HostedPlaneError};
use hh_subagent::spawn::spawn;

/// A test-double plane: records `open_child` specs, admits only the verbs
/// the (pretend) participant's capability vector declares (`submit`,
/// `stream_events`), refuses everything else typed — the boundary behavior
/// AC-R-2.8.1-4's hosted clause pins.
struct FakePlane {
    opened: Vec<Json>,
    cancelled: Vec<String>,
    covered: BTreeSet<String>,
    unresolved: bool,
}

impl FakePlane {
    fn new() -> FakePlane {
        FakePlane {
            opened: Vec::new(),
            cancelled: Vec::new(),
            covered: BTreeSet::from([
                "submit".to_string(),
                "stream_events".to_string(),
                "cancel".to_string(),
            ]),
            unresolved: false,
        }
    }
}

impl HostedPlane for FakePlane {
    fn open_child(&mut self, spec: &Json) -> Result<HostedOpen, HostedPlaneError> {
        if self.unresolved {
            return Err(HostedPlaneError::ProcessUnresolvable {
                detail: "process.hosted does not resolve to an OpaqueProcess".into(),
            });
        }
        self.opened.push(spec.clone());
        Ok(HostedOpen {
            session_ref: format!(
                "sess-{}",
                spec.get("child_run_id")
                    .and_then(Json::as_str)
                    .unwrap_or("?")
            ),
            hosting_mechanism: "session_abi".to_string(),
            // `open_run` validates manifest refs resolve — a real plane
            // returns the sealed declaration's ref; the double leaves it
            // unset.
            capability_declaration_ref: None,
        })
    }

    fn send_verb(
        &mut self,
        _session: &str,
        verb: &str,
        _params: &Json,
    ) -> Result<Json, HostedPlaneError> {
        // The boundary's coverage decision: undeclared ⇒ `unknown`, never
        // granted (ADR-0053 D-4); ABI-foreign spellings are UnknownVerb.
        match verb {
            "rm_rf_root" | "drop_table" => Err(HostedPlaneError::UnknownVerb { verb: verb.into() }),
            v if self.covered.contains(v) => Ok(Json::obj([("ok", Json::Bool(true))])),
            v => Err(HostedPlaneError::UncoveredCall { verb: v.into() }),
        }
    }

    fn cancel(&mut self, session: &str) -> Result<Json, HostedPlaneError> {
        self.cancelled.push(session.to_string());
        Ok(Json::obj([("cancelled", Json::Bool(true))]))
    }
}

fn hosted_spec() -> SubagentSpec {
    let mut s = spec();
    s.process = ChildProcess::Hosted(Ref::pinned(
        "participant:ext",
        format!("sha256:{}", "e".repeat(64)),
    ));
    s
}

#[test]
fn hosted_child_composes_through_the_plane_with_hosted_manifest() {
    let mut p = open_parent("hosted-ok");
    let mut plane = FakePlane::new();
    let spawned = {
        let mut c = ctx(&mut p);
        c.hosted_plane = Some(&mut plane);
        spawn(&mut c, &hosted_spec()).unwrap()
    };
    // The plane saw exactly one open, keyed on the deterministic child id.
    assert_eq!(plane.opened.len(), 1);
    assert_eq!(
        plane.opened[0].get("child_run_id").and_then(Json::as_str),
        Some(spawned.child_run_id.as_str())
    );
    // The spawned row carries the durable boundary mapping.
    let ev = p
        .store
        .events(&p.run_id)
        .unwrap()
        .iter()
        .find(|e| e.class == "control.subagent.spawned")
        .unwrap();
    let hosted = ev.payload.get("hosted").expect("hosted member");
    assert_eq!(
        hosted.get("session_ref").and_then(Json::as_str),
        Some(format!("sess-{}", spawned.child_run_id).as_str())
    );
    assert_eq!(
        hosted.get("hosting_mechanism").and_then(Json::as_str),
        Some("session_abi")
    );
    // The child manifest is the hosted class with the plane-resolved
    // mechanism and capability declaration (AC-R-2.1.5-6's class stamp).
    let m = p.store.manifest(&spawned.child_run_id).unwrap();
    assert_eq!(
        m.participant_class,
        hh_ledger::manifest::ParticipantClass::Hosted
    );
    assert_eq!(m.hosting_mechanism.as_deref(), Some("session_abi"));
    assert_eq!(m.capability_declaration_ref, None);
}

#[test]
fn hosted_child_without_a_plane_is_mode_unsupported() {
    let mut p = open_parent("hosted-noplane");
    let mut c = ctx(&mut p);
    match spawn(&mut c, &hosted_spec()) {
        Err(SpawnError::Refused(SpawnRefused::ModeUnsupported { detail })) => {
            assert!(detail.contains("hosted plane"), "{detail}");
        }
        other => panic!("expected ModeUnsupported, got {other:?}"),
    }
}

#[test]
fn hosted_process_ref_must_be_pinned() {
    let mut p = open_parent("hosted-selector");
    let mut plane = FakePlane::new();
    let mut s = hosted_spec();
    s.process = ChildProcess::Hosted(Ref::selected("participant:ext", "latest"));
    let mut c = ctx(&mut p);
    c.hosted_plane = Some(&mut plane);
    match spawn(&mut c, &s) {
        Err(SpawnError::Refused(SpawnRefused::DefinitionUnresolvable { detail })) => {
            assert!(detail.contains("not pinned"), "{detail}");
        }
        other => panic!("expected DefinitionUnresolvable, got {other:?}"),
    }
    assert!(plane.opened.is_empty());
}

#[test]
fn hosted_process_unresolvable_is_a_definition_error() {
    let mut p = open_parent("hosted-unresolvable");
    let mut plane = FakePlane::new();
    plane.unresolved = true;
    let mut c = ctx(&mut p);
    c.hosted_plane = Some(&mut plane);
    match spawn(&mut c, &hosted_spec()) {
        Err(SpawnError::Refused(SpawnRefused::DefinitionUnresolvable { detail })) => {
            assert!(detail.contains("does not resolve"), "{detail}");
        }
        other => panic!("expected DefinitionUnresolvable, got {other:?}"),
    }
    // The plane refusal landed before the spawned row — no child scope.
    assert!(p.store.run_ids().is_empty() || p.store.run_ids().len() == 1);
}

#[test]
fn hosted_uncovered_call_is_refused_at_the_boundary() {
    // C-10 / AC-R-2.8.1-4: the plane refuses what the declared capability
    // vector does not cover — `submit` is declared, `fs_write` is not.
    let mut plane = FakePlane::new();
    assert!(plane.send_verb("sess-1", "submit", &Json::Null).is_ok());
    match plane.send_verb("sess-1", "fs_write", &Json::Null) {
        Err(HostedPlaneError::UncoveredCall { verb }) => assert_eq!(verb, "fs_write"),
        other => panic!("expected UncoveredCall, got {other:?}"),
    }
    match plane.send_verb("sess-1", "rm_rf_root", &Json::Null) {
        Err(HostedPlaneError::UnknownVerb { verb }) => assert_eq!(verb, "rm_rf_root"),
        other => panic!("expected UnknownVerb, got {other:?}"),
    }
    // C-10's failure shape: `cancel` at the boundary, never a reach inside.
    assert!(plane.cancel("sess-1").is_ok());
    assert_eq!(plane.cancelled, vec!["sess-1".to_string()]);
}
