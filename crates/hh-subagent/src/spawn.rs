//! The C1 kernel operation `spawn` (§5e.3 `R-2.6.3¹`; ADR-0185 D2; ADR-0193
//! D4 realised inside it, CF-451).
//!
//! Steps (atomic; idempotent on `(decision, H(spec))`; every durable step
//! lands under the parent's writer lease):
//!
//! 1. envelope checks — `delegation_depth + 1 ≤ cap`, `fan_out + 1 ≤ cap`,
//!    `reserve(spawns, 1)` on the parent's budget node.
//! 2. the `spawn_process`-domain admission — realized as the attenuation
//!    check `delegate` runs in step 3 (the Π row "allow iff attenuation
//!    holds" *is* the delegate check; there is no second authority —
//!    ADR-0052/OQ-135). Static spec checks (`DefinitionUnresolvable`) run
//!    first.
//! 3. `delegate(parent, child_spec, requested_grants, budget_req, at)` —
//!    `hh-monitor`'s one attenuation seam (ADR-0053 D-1/D-2).
//! 4. `allocate(parent_budget, spec.budget.spec, mode)` (ADR-0040) — the
//!    child `BudgetNode` under `slice`/`pool` (R-2.1.6¹).
//!    - `grant_ownership(ownership_grants[])` ⊆ the parent's valid ownerships
//!      (ADR-0191 O-2) — `control.ownership.transferred` rows.
//!    - `isolation.environment = share` is declared but refused
//!      `ModeUnsupported` at this slice (Stage-5 arm; `reserved_keys[]`
//!      still ledger on `spawned`).
//! 5. `derive(parent_env, mode, spec)` when requested (ADR-0138).
//! 6. `control.subagent.spawned` lands **before** the child's `open_run`:
//!    the child's manifest `spawn_event` is the delegated-to edge — the
//!    spawned row itself — and `open_run` validates it resolves. The
//!    deterministic `sub-<H(decision, H(spec))>` `RunId` makes the pair
//!    one idempotent unit: a crash between the spawned row and
//!    `open_run` is recovered as `cancelled{infrastructure_failure}` by
//!    [`crate::recovery::recover_parent`]; a crash before it leaves only
//!    ttl-dying reservations (KP-21).
//! 7. The child run opens under its own writer lease.
//! 8. `subscribe(parent, child_terminal{child})` — `wait.mode = await`
//!    returns the `delegation_completed` wait cue for the caller to
//!    `suspend{awaiting_child}` (or drive `drain_child_terminal`).

use std::collections::BTreeMap;

use hh_budget::account::Account;
use hh_budget::quantity::ResourceVector;
use hh_budget::spec::BudgetScope;
use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::manifest::EventRef;
use hh_ledger::manifest::RunManifest;
use hh_ledger::store::{Lease, Store};
use hh_ledger::LedgerError;
use hh_monitor::delegate::{delegate, ChildSpec, LiveCoords};
use hh_monitor::handle::{AuthorityHandle, HandleId};
use hh_monitor::table::HandleTable;
use hh_ontology::control::Owner;
use hh_ontology::dimensions::DimensionId;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::ownership::OwnershipTable;
use crate::types::*;

/// The kernel producer tag for subagent rows (one producer — CC1).
pub const KERNEL_SUBAGENT: &str = "kernel.subagent";

/// The deterministic child `RunId` — `sub-<idp(decision_ref, spec_hash)>`:
/// the `(decision, H(spec))` idempotency pair *is* the id (SP/safe-retry).
pub fn child_run_id_for(decision: &EventRef, spec_hash: &str) -> String {
    let preimage = Json::Arr(vec![
        Json::str(format!("{}:{}", decision.run_id, decision.event_id)),
        Json::str(spec_hash),
    ]);
    format!(
        "sub-{}",
        hh_identity::idp::idp_id(
            "hh.subagent.spawn",
            preimage.to_canonical_string().as_bytes()
        )
    )
}

/// The spawn phases the [`SpawnHook`] observes — the KP-14/16–21 fault
/// seams (a hook returning `Err` is a kill at the durable boundary it was
/// called on: everything before is durable; nothing after lands).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpawnPhase {
    /// After step-1 envelope checks (before `reserve`).
    AfterEnvelopeChecks,
    /// After `reserve(spawns, 1)` — KP-21's early boundary.
    AfterReserve,
    /// After `delegate` (in-memory handles; nothing durable yet).
    AfterDelegate,
    /// After `allocate` (the child `BudgetNode` is durable).
    AfterAllocate,
    /// After `control.ownership.transferred` rows.
    AfterOwnership,
    /// After `derive` (the `environment.derived` row is durable).
    AfterDerive,
    /// After `control.subagent.spawned` is durable — the delegated-to edge
    /// exists; `open_run` has not run (recovery marks this child
    /// `cancelled{infrastructure_failure}` if the run never lands).
    AfterSpawnedAppend,
    /// After the child's `open_run` + writer lease — the child exists.
    AfterChildOpen,
    /// After `subscribe(parent, child_terminal{child})`.
    AfterSubscribe,
}

/// A fault-injection seam — `None` in production. `at(phase)` runs at each
/// durable boundary; an `Err` return aborts the spawn at that boundary
/// (the KP-battery's "runtime death" — the store is simply dropped next).
pub trait SpawnHook {
    /// Called at each [`SpawnPhase`].
    fn at(&mut self, phase: SpawnPhase) -> Result<(), SpawnError>;
}

/// What `spawn` needs beyond its arguments — the parent's seam set. All
/// ledgered steps run under `parent_lease` on `parent_run_id`'s store.
pub struct SpawnCtx<'a> {
    /// The ledger store holding both runs.
    pub store: &'a mut Store,
    /// The parent's authority table (the `delegate` input — in-memory; the
    /// minted child set is recorded on the `spawned` row).
    pub handles: &'a HandleTable,
    /// The parent run.
    pub parent_run_id: &'a str,
    /// The parent's writer lease (every step commits under it).
    pub parent_lease: &'a Lease,
    /// The parent handle the child delegates from.
    pub parent_handle_id: &'a HandleId,
    /// The parent's `BudgetNode` id the child node allocates under.
    pub parent_budget_id: &'a str,
    /// The parent's live environment handle (for `isolation.environment =
    /// derive{…}`); `None` refuses `Derive` requests `ModeUnsupported`.
    pub parent_env_id: Option<&'a str>,
    /// The environment driver (the `derive` port; `None` refuses `Derive`).
    pub env_driver: Option<&'a mut (dyn EnvDeriver + 'static)>,
    /// The parent's valid ownerships (step 4b's ⊆ check).
    pub parent_ownerships: &'a OwnershipTable,
    /// The parent's sealed tool table (`supplies.tools ⊆`).
    pub parent_tool_table: &'a [hh_hir::refs::Ref],
    /// The parent's own delegation depth (`delegation_depth` gauge level).
    pub parent_depth: u64,
    /// The `control.decision{kind: delegate}` this spawn hangs from
    /// (`causes[]` + `delegation_ref` + the idempotency key's decision half).
    pub decision: EventRef,
    /// The decision's owner stamp (`model` ⇒ the `delegation_reason` is a
    /// `model_claim` at `delegate`; the envelope already refused a missing
    /// reason).
    pub owner: Owner,
    /// The lease-holder spelling (writer acquisition for the child).
    pub holder: &'a str,
    /// The live liveness coordinates `delegate` checks the parent handle
    /// against.
    pub coords: LiveCoords,
    /// The reservation TTL for `reserve(spawns, 1)` (spec: reservations die
    /// with the lease — pass the lease's remaining ms).
    pub reserve_ttl_ms: u64,
    /// The fault-injection seam (KP-14/16–21).
    pub hook: Option<&'a mut (dyn SpawnHook + 'static)>,
}

/// The `derive` port — implemented by `hh_env::driver::EnvDriver` (and test
/// doubles); returns the child's `env_handle_id`.
pub trait EnvDeriver {
    /// `derive(parent_env, mode, scope, on_parent_end, derived_for) →
    /// env_handle_id` under the parent's lease (the `environment.derived`
    /// row lands on the parent run — step 5 is ledgered under the parent's
    /// writer lease).
    ///
    /// The parameters stay explicit because each names a distinct authority
    /// seam (store, lease, parent handle, mode, scope, lifecycle and the
    /// deterministic child id); packing them would hide that surface.
    #[allow(clippy::too_many_arguments)]
    fn derive(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        parent_env_id: &str,
        mode: hh_env::handle::DeriveMode,
        scope: Option<&str>,
        on_parent_end: hh_env::handle::OnParentEnd,
        derived_for: &str,
    ) -> Result<String, String>;
}

impl EnvDeriver for hh_env::driver::EnvDriver {
    #[allow(clippy::too_many_arguments)]
    fn derive(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        parent_env_id: &str,
        mode: hh_env::handle::DeriveMode,
        scope: Option<&str>,
        on_parent_end: hh_env::handle::OnParentEnd,
        derived_for: &str,
    ) -> Result<String, String> {
        let h = hh_env::driver::EnvDriver::derive_for(
            self,
            store,
            lease,
            parent_env_id,
            mode,
            scope,
            on_parent_end,
            Some(derived_for),
        )
        .map_err(|e| e.to_string())?;
        Ok(h.env_handle_id)
    }
}

/// Mint an unstaged kernel `Event` under the parent run (the append path
/// stamps `parent_event_id`? — no: the caller passes the current head; one
/// producer, `KERNEL_SUBAGENT`).
/// The kernel event mint — shared with the sibling modules (one producer).
pub(crate) fn kernel_ev_pub(
    store: &Store,
    run_id: &str,
    class: &str,
    payload: Json,
    causes: Vec<EventRef>,
    provenance: Option<ProvenanceRecord>,
) -> Result<Event, LedgerError> {
    Ok(Event {
        event_id: store.alloc_id("evt"),
        class: class.to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel(KERNEL_SUBAGENT),
        scope: Scope::default(),
        parent_event_id: store.head_event_id(run_id)?,
        causes,
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: provenance
            .or_else(|| Some(ProvenanceRecord::kernel(KERNEL_SUBAGENT, store.now_ms()))),
        content_kind: None,
        payload,
    })
}

/// The `spawned` union payload (§5e.3 Ledger paragraph — every spawn path
/// carries the one schema).
#[allow(clippy::too_many_arguments)]
fn spawned_payload(
    child_run_id: &str,
    delegation_ref: &EventRef,
    child_handles: &[String],
    parent_handle: &HandleId,
    ceiling: hh_provenance::AuthorityClass,
    attenuation_delta: &Json,
    budget_id: &str,
    spec: &SubagentSpec,
    env_handle_id: Option<&str>,
    owner: Owner,
    depth: u64,
    parent_head: &Json,
    reservation_id: &str,
    messaging_policy: Option<&MessagingPolicy>,
) -> Json {
    let mut m = vec![
        ("child_run_id", Json::str(child_run_id)),
        (
            "delegation_ref",
            Json::str(format!(
                "{}:{}",
                delegation_ref.run_id, delegation_ref.event_id
            )),
        ),
        (
            "child_handles",
            Json::Arr(
                child_handles
                    .iter()
                    .map(|s| Json::str(s.as_str()))
                    .collect(),
            ),
        ),
        ("parent_handle", Json::str(parent_handle.as_str())),
        ("ceiling", Json::str(ceiling.as_str())),
        ("attenuation_delta", attenuation_delta.clone()),
        ("budget_id", Json::str(budget_id)),
        ("reservation_id", Json::str(reservation_id)),
        ("mode", Json::str(budget_mode_str(spec.budget_mode))),
        ("isolation_mode", Json::str(spec.environment.mode_str())),
        (
            "supplies_digest",
            Json::str(supplies_digest(&spec.supplies)),
        ),
        (
            "return_contract_ref",
            Json::str(spec.return_contract.contract_ref()),
        ),
        (
            "delegation_reason",
            Json::str(spec.delegation_reason.as_str()),
        ),
        ("owner", Json::str(owner_str(owner))),
        ("wait_mode", Json::str(spec.wait_mode.as_str())),
        (
            "on_parent_end",
            Json::str(on_parent_end_str(spec.on_parent_end)),
        ),
        ("depth", Json::Int(depth.min(i64::MAX as u64) as i64)),
        ("parent_head_at_spawn", parent_head.clone()),
        (
            "ownership_grants",
            Json::Arr(
                spec.ownership_grants
                    .iter()
                    .map(OwnedObject::to_json)
                    .collect(),
            ),
        ),
        (
            "reserved_keys",
            Json::Arr(
                spec.reserved_keys
                    .iter()
                    .map(|s| Json::str(s.as_str()))
                    .collect(),
            ),
        ),
        (
            "consistency_declarations",
            Json::Arr(spec.consistency_declarations.clone()),
        ),
    ];
    if let Some(e) = env_handle_id {
        m.push(("env_handle_id", Json::str(e)));
    }
    if let Some(t) = &spec.topology_ref {
        m.push(("topology_ref", Json::str(t.clone())));
    }
    if let Some(i) = spec.stage_index {
        m.push(("stage_index", Json::Int(i.min(i64::MAX as u64) as i64)));
    }
    match &spec.merge_policy_ref {
        Some(mp) => m.push(("merge_policy_ref", Json::str(mp.clone()))),
        None => m.push(("merge_policy_ref", Json::Null)),
    }
    if let Some(p) = messaging_policy {
        m.push(("messaging_policy", p.to_json()));
    }
    Json::obj(m)
}

/// The `supplies_digest` — idp/1 over the canonical supplies form (the
/// spawned row names supplied content by digest, never by value).
fn supplies_digest(s: &Supplies) -> String {
    hh_identity::idp::idp_id(
        "hh.subagent.supplies",
        s.to_json().to_canonical_string().as_bytes(),
    )
}

/// A spawned-row fold record (what the parent ledger knows about one
/// child).
#[derive(Debug, Clone)]
pub struct ChildRecord {
    /// The spawned payload (canonical JSON).
    pub spawned: Json,
    /// The `control.subagent.spawned` event id.
    pub spawn_event_id: String,
    /// The `control.subagent.spawned` seq.
    pub spawn_seq: u64,
    /// The terminal class seen (`result`/`cancelled`/`detached`), if any.
    pub terminal_class: Option<String>,
    /// The terminal event's payload, if any.
    pub terminal_payload: Option<Json>,
}

/// Fold the parent's subagent rows into per-child records (spawn order).
/// The run-tree projection's local half — `parent_run_id`/`spawn_event`
/// edges, one child per `spawned`.
pub fn fold_children(
    store: &Store,
    parent_run_id: &str,
) -> Result<Vec<(String, ChildRecord)>, SpawnError> {
    let events = store
        .events(parent_run_id)
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    let mut order: Vec<String> = Vec::new();
    let mut map: BTreeMap<String, ChildRecord> = BTreeMap::new();
    for e in events {
        let child = e
            .payload
            .get("child_run_id")
            .and_then(Json::as_str)
            .map(str::to_string);
        match e.class.as_str() {
            "control.subagent.spawned" => {
                if let Some(c) = child {
                    order.push(c.clone());
                    map.insert(
                        c,
                        ChildRecord {
                            spawned: e.payload.clone(),
                            spawn_event_id: e.event_id.clone(),
                            spawn_seq: e.seq,
                            terminal_class: None,
                            terminal_payload: None,
                        },
                    );
                }
            }
            "control.subagent.result"
            | "control.subagent.cancelled"
            | "control.subagent.detached" => {
                if let Some(c) = child {
                    if let Some(rec) = map.get_mut(&c) {
                        if rec.terminal_class.is_none() {
                            rec.terminal_class = Some(e.class.clone());
                            rec.terminal_payload = Some(e.payload.clone());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(order
        .into_iter()
        .filter_map(|c| map.get(&c).map(|r| (c, r.clone())))
        .collect())
}

/// The live-children count — spawned minus terminally recorded (`fan_out`
/// decrement at the child's terminal, §5e.3 step-8 note). `detached`
/// children no longer count against the *parent's* fan-out (T6 — they
/// detached; the cap tracks parent-mediated children).
pub fn live_children(store: &Store, parent_run_id: &str) -> Result<u64, SpawnError> {
    Ok(fold_children(store, parent_run_id)?
        .iter()
        .filter(|(_, r)| r.terminal_class.is_none())
        .count() as u64)
}

/// `spawn(ctx, spec) → Spawned | SpawnRefused` — the C1 slice's one kernel
/// operation. Everything the caller must supply is declared in
/// [`SpawnCtx`]; the idempotent retry path is inside.
pub fn spawn(ctx: &mut SpawnCtx, spec: &SubagentSpec) -> Result<Spawned, SpawnError> {
    macro_rules! hook_phase {
        ($ctx:expr, $p:expr) => {{
            if let Some(h) = $ctx.hook.as_deref_mut() {
                h.at($p)?;
            }
        }};
    }
    let spec_hash = spec.spec_hash();
    let child_run_id = child_run_id_for(&ctx.decision, &spec_hash);

    // ── idempotent replay ────────────────────────────────────────────────
    // A `spawned` row already naming this `(decision, H(spec))` id means a
    // prior attempt got at least as far as step 7 — adopt it: re-run
    // nothing that could double (no second `spawned`, no second reserve).
    if let Some(existing) = fold_children(ctx.store, ctx.parent_run_id)?
        .into_iter()
        .find(|(c, _)| *c == child_run_id)
    {
        let rec = existing.1;
        if rec.terminal_class.is_some() {
            return Err(SpawnError::Kernel(format!(
                "spawn for {child_run_id} is already terminal ({})",
                rec.terminal_class.as_deref().unwrap_or("unknown")
            )));
        }
        let spawn_event = EventRef {
            run_id: ctx.parent_run_id.to_string(),
            event_id: rec.spawn_event_id.clone(),
        };
        // The child may not exist (crash between the spawned row and
        // open_run — KP-21's late half): finish the interrupted spawn.
        let replay_anchor = rec
            .spawned
            .get("parent_head_at_spawn")
            .cloned()
            .unwrap_or_else(|| {
                let head = ctx
                    .store
                    .head(ctx.parent_run_id)
                    .expect("parent head exists");
                Json::obj([
                    ("seq", Json::Int(head.seq as i64)),
                    ("hash", Json::str(head.hash.as_str())),
                ])
            });
        let replay_lease = if !ctx.store.has_run(&child_run_id) {
            finish_child_open(ctx, spec, &child_run_id, &spawn_event, &replay_anchor)?
        } else {
            None
        };
        let child_handles = rec
            .spawned
            .get("child_handles")
            .and_then(|j| match j {
                Json::Arr(a) => Some(
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default();
        let budget_id = rec
            .spawned
            .get("budget_id")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        let env_handle_id = rec
            .spawned
            .get("env_handle_id")
            .and_then(Json::as_str)
            .map(str::to_string);
        // The `child_terminal` subscription: replay finds it when the kill
        // landed after `AfterSubscribe`; an earlier kill (AfterSpawnedAppend
        // / AfterChildOpen — KP-16/21) leaves none and adoption creates it
        // under the parent's lease (`created_by` = the spawn event — the
        // same shape the first pass produces).
        let subscription_id = match subscription_for(ctx.store, ctx.parent_run_id, &child_run_id) {
            Ok(id) => id,
            Err(_) => ctx
                .store
                .wakeup_subscribe(
                    ctx.parent_run_id,
                    ctx.parent_lease,
                    hh_ledger::wakeup::Trigger::ChildTerminal {
                        child_run_id: child_run_id.clone(),
                    },
                    hh_ledger::wakeup::WakeupPolicy::default_policy(),
                    &spawn_event,
                )
                .map_err(|e| SpawnError::Kernel(format!("child_terminal subscribe: {e}")))?,
        };
        // The original attempt's `spawns` reservation is spent by the same
        // spawned row on replay. Its id rides the spawned payload so the
        // adoption path frees exactly this reservation rather than waiting
        // for lease expiry (KP-21's nothing-held rule under a live lease).
        if let Some(reservation_id) = rec.spawned.get("reservation_id").and_then(Json::as_str) {
            let mut account = Account::open(ctx.store, ctx.parent_run_id)
                .map_err(|e| SpawnError::Kernel(format!("account open: {e}")))?;
            let _ = account.release(ctx.parent_lease, reservation_id);
        }
        let child_lease = replay_lease.or_else(|| {
            ctx.store
                .acquire_writer(
                    ctx.holder,
                    &child_run_id,
                    ctx.parent_lease
                        .expires_at_ms
                        .saturating_sub(ctx.store.now_ms()),
                )
                .ok()
        });
        return Ok(Spawned {
            child_run_id,
            child_handles,
            budget_id,
            env_handle_id,
            spawn_event,
            child_lease,
            wait_mode: spec.wait_mode,
            subscription_id,
        });
    }

    // ── step 2 pre-checks (the spec as data — SP-7 sealed shape) ────────
    if let Some(detail) = spec.definition_errors().into_iter().next() {
        return Err(SpawnError::Refused(SpawnRefused::DefinitionUnresolvable {
            detail,
        }));
    }
    // `supplies.tools ⊆` the parent's sealed tool table (SP-3).
    for t in &spec.supplies.tools {
        if !ctx.parent_tool_table.iter().any(|p| p == t) {
            return Err(SpawnError::Refused(SpawnRefused::DefinitionUnresolvable {
                detail: "supplies.tools is not a subset of the sealed tool table".into(),
            }));
        }
    }
    // `share` isolation — declared, Stage-5 arm (4c's resource-lock walk is
    // the share machinery's; the keys still record on `spawned`).
    if matches!(spec.environment, EnvIsolation::Derive { mode, .. } if mode == hh_env::handle::DeriveMode::Share)
    {
        return Err(SpawnError::Refused(SpawnRefused::ModeUnsupported {
            detail: "isolation.environment = share is the Stage-5 arm".into(),
        }));
    }
    if matches!(spec.environment, EnvIsolation::Derive { .. })
        && (ctx.parent_env_id.is_none() || ctx.env_driver.is_none())
    {
        return Err(SpawnError::Refused(SpawnRefused::ModeUnsupported {
            detail: "derive requested without a live parent environment / driver".into(),
        }));
    }

    // ── step 1 — envelope checks ─────────────────────────────────────────
    // `fan_out + 1 ≤ cap` reads the live-children fold before the account
    // borrows `ctx.store` mutably.
    let live = live_children(ctx.store, ctx.parent_run_id)?;
    let reservation_id;
    {
        let mut account = Account::open(ctx.store, ctx.parent_run_id)
            .map_err(|e| SpawnError::Kernel(format!("account open: {e}")))?;
        // `delegation_depth + 1 ≤ cap` (a gauge cap on the parent's node —
        // refused without side effects before any reservation).
        account
            .gauge_reserve(
                ctx.parent_budget_id,
                DimensionId::DelegationDepth,
                (ctx.parent_depth + 1).min(i64::MAX as u64) as i64,
            )
            .map_err(|_| {
                SpawnError::Refused(SpawnRefused::Depth {
                    depth: ctx.parent_depth + 1,
                    cap: ctx.parent_depth,
                })
            })?;
        // `fan_out + 1 ≤ cap`.
        account
            .gauge_reserve(
                ctx.parent_budget_id,
                DimensionId::FanOut,
                (live + 1).min(i64::MAX as u64) as i64,
            )
            .map_err(|_| {
                SpawnError::Refused(SpawnRefused::FanOut {
                    dimension: "fan_out".into(),
                })
            })?;
        hook_phase!(ctx, SpawnPhase::AfterEnvelopeChecks);

        // `reserve(spawns, 1)` — the feedback-path bound (INV-6). The
        // holder names the deterministic child id so a KP-21 retry reuses
        // the outstanding reservation instead of taking a second claim;
        // `spawned` records the same id and the adoption path releases it.
        let spawn_holder = format!("subagent_spawn:{child_run_id}");
        reservation_id = match account
            .tree
            .reservation_for(ctx.parent_budget_id, &spawn_holder)
            .map(|r| r.reservation_id.clone())
        {
            Some(existing) => existing,
            None => account
                .reserve(
                    ctx.parent_lease,
                    ctx.parent_budget_id,
                    &ResourceVector::one(DimensionId::Spawns, 1),
                    &spawn_holder,
                    ctx.reserve_ttl_ms,
                )
                .map_err(|e| match e {
                    hh_budget::BudgetError::InsufficientBudget { dimension, .. } => {
                        SpawnError::Refused(SpawnRefused::InsufficientBudget {
                            dimension: dimension.to_string(),
                        })
                    }
                    other => SpawnError::Kernel(format!("reserve spawns: {other}")),
                })?,
        };
    }
    hook_phase!(ctx, SpawnPhase::AfterReserve);

    // ── step 3 — delegate (the Π `spawn_process` row: allow iff
    //    attenuation holds; one attenuation seam — ADR-0053 D-1/D-2) ──────
    let child_spec = ChildSpec {
        holder: hh_hir::refs::Ref::pinned("agent_process".to_string(), child_run_id.clone()),
        requested: spec.requested_grants.clone(),
        ceiling: spec
            .ceiling
            .unwrap_or(hh_provenance::AuthorityClass::Delegate),
        budget_req: (!spec.budget_spec.hard_caps_map().is_empty()).then(|| {
            // The delegate-time budget bound is the child's spec's hard caps
            // as a Json object `{dim: amount}`.
            let mut o = BTreeMap::new();
            for (k, v) in spec.budget_spec.hard_caps_map() {
                o.insert(k, Json::Int(v));
            }
            Json::Obj(o)
        }),
    };
    let child_handles: Vec<AuthorityHandle> = {
        let mut alloc = |kind: &str| ctx.store.alloc_id(kind);
        delegate(
            ctx.handles,
            ctx.parent_handle_id,
            &child_spec,
            &ctx.coords,
            &mut alloc,
        )
        .map_err(|e| {
            use hh_monitor::delegate::DelegateError::*;
            SpawnError::Refused(match e {
                AuthorityWidening { domain } => SpawnRefused::AuthorityWidening {
                    domain: domain.name().to_string(),
                },
                NotDelegable { handle_id } => SpawnRefused::NotDelegable { handle: handle_id },
                BudgetExceedsParent => SpawnRefused::BudgetExceedsParent {
                    detail: "budget_req exceeds the covering grants' constraints".into(),
                },
                CeilingExceeded => SpawnRefused::AuthorityWidening {
                    domain: "ceiling".into(),
                },
                ParentNotLive { handle_id } => SpawnRefused::NotDelegable { handle: handle_id },
            })
        })?
    };
    hook_phase!(ctx, SpawnPhase::AfterDelegate);

    // ── step 4 — allocate the child BudgetNode (R-2.1.6¹ slice/pool) ─────
    let child_budget_id;
    {
        let mut account = Account::open(ctx.store, ctx.parent_run_id)
            .map_err(|e| SpawnError::Kernel(format!("account open: {e}")))?;
        // `scope.target = child_run_id` is the allocation's idempotency
        // key: a KP-21 retry after `control.budget.allocated` but before
        // `spawned` adopts the same node (slice claims therefore never
        // double-charge the parent).
        child_budget_id = account
            .tree
            .nodes
            .values()
            .find(|n| {
                n.node.scope.kind == hh_budget::spec::BudgetScopeKind::AgentProcess
                    && n.node.scope.target == child_run_id
            })
            .map(|n| n.node.budget_id.clone())
            .map(Ok)
            .unwrap_or_else(|| {
                account.allocate(
                    ctx.parent_lease,
                    Some(ctx.parent_budget_id),
                    BudgetScope {
                        kind: hh_budget::spec::BudgetScopeKind::AgentProcess,
                        target: child_run_id.clone(),
                    },
                    spec.budget_spec.clone(),
                )
            })
            .map_err(|e| match e {
                hh_budget::BudgetError::InsufficientBudget { dimension, .. }
                | hh_budget::BudgetError::BudgetExceedsParent { dimension, .. } => {
                    SpawnError::Refused(SpawnRefused::BudgetExceedsParent {
                        detail: format!("{dimension} exceeds the parent's remaining"),
                    })
                }
                other => SpawnError::Kernel(format!("allocate: {other}")),
            })?;
    }
    hook_phase!(ctx, SpawnPhase::AfterAllocate);

    // ── step 4b — ownership grants ⊆ the parent's valid ownerships ───────
    let mut transferred: Vec<Event> = Vec::new();
    let mut seen_grants = std::collections::BTreeSet::new();
    for obj in &spec.ownership_grants {
        if !seen_grants.insert(obj.to_json().to_canonical_string()) {
            return Err(SpawnError::Refused(SpawnRefused::DefinitionUnresolvable {
                detail: "ownership_grants contains a duplicate object".into(),
            }));
        }
        if !ctx.parent_ownerships.holds(ctx.parent_run_id, obj) {
            return Err(SpawnError::Refused(SpawnRefused::NotOwned {
                object: obj.to_json(),
            }));
        }
        // `(object, from, to, grant_of)` is the transfer's idempotency
        // key. A kill after `AfterOwnership` but before `spawned` must not
        // mint a second transfer row on retry (KP-21 no duplicate effect).
        let already_transferred = ctx
            .store
            .events(ctx.parent_run_id)
            .map_err(|e| SpawnError::Kernel(e.to_string()))?
            .iter()
            .any(|e| {
                e.class == "control.ownership.transferred"
                    && e.payload.get("object") == Some(&obj.to_json())
                    && e.payload.get("from").and_then(Json::as_str) == Some(ctx.parent_run_id)
                    && e.payload.get("to").and_then(Json::as_str) == Some(child_run_id.as_str())
                    && e.payload.get("grant_of").and_then(Json::as_str)
                        == Some(ctx.decision.event_id.as_str())
            });
        if already_transferred {
            continue;
        }
        transferred.push(
            kernel_ev_pub(
                ctx.store,
                ctx.parent_run_id,
                "control.ownership.transferred",
                Json::obj([
                    ("object", obj.to_json()),
                    ("from", Json::str(ctx.parent_run_id)),
                    ("to", Json::str(child_run_id.as_str())),
                    ("grant_of", Json::str(ctx.decision.event_id.as_str())),
                ]),
                vec![ctx.decision.clone()],
                None,
            )
            .map_err(|e| SpawnError::Kernel(e.to_string()))?,
        );
    }
    if !transferred.is_empty() {
        ctx.store
            .append(ctx.parent_run_id, ctx.parent_lease, transferred)
            .map_err(|e| SpawnError::Kernel(format!("ownership rows: {e}")))?;
    }
    hook_phase!(ctx, SpawnPhase::AfterOwnership);

    // ── step 5 — derive the child environment when requested ────────────
    let env_handle_id = match &spec.environment {
        EnvIsolation::None => None,
        EnvIsolation::Derive { mode, spec: dspec } => {
            let scope = dspec.get("scope").and_then(Json::as_str);
            let driver = ctx.env_driver.as_deref_mut().ok_or_else(|| {
                SpawnError::Refused(SpawnRefused::ModeUnsupported {
                    detail: "derive requested without a driver".into(),
                })
            })?;
            let parent_env = ctx.parent_env_id.ok_or_else(|| {
                SpawnError::Refused(SpawnRefused::ModeUnsupported {
                    detail: "derive requested without a parent environment".into(),
                })
            })?;
            // `derived_for = child_run_id` is the derivation's replay key:
            // a kill after `AfterDerive` but before `spawned` adopts the
            // durable handle instead of minting a second environment.
            let prior = ctx
                .store
                .events(ctx.parent_run_id)
                .map_err(|e| SpawnError::Kernel(e.to_string()))?
                .iter()
                .find(|e| {
                    e.class == "action.environment.derived"
                        && e.payload.get("derived_for").and_then(Json::as_str)
                            == Some(child_run_id.as_str())
                })
                .and_then(|e| {
                    e.payload
                        .get("env_handle")
                        .or_else(|| e.payload.get("child_env"))
                        .and_then(Json::as_str)
                        .map(str::to_string)
                });
            Some(match prior {
                Some(handle) => handle,
                None => driver
                    .derive(
                        ctx.store,
                        ctx.parent_lease,
                        parent_env,
                        *mode,
                        scope,
                        spec.on_parent_end,
                        &child_run_id,
                    )
                    .map_err(SpawnError::Kernel)?,
            })
        }
    };
    hook_phase!(ctx, SpawnPhase::AfterDerive);

    // ── step 7a — `control.subagent.spawned` (the delegated-to edge) ────
    // Lands BEFORE `open_run`: the child's `spawn_event` is this row and
    // `open_run` validates it resolves. The union payload carries the
    // child_run_id it names — the deterministic id binds the pair.
    let head = ctx
        .store
        .head(ctx.parent_run_id)
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    let parent_head = Json::obj([
        ("seq", Json::Int(head.seq as i64)),
        ("hash", Json::str(head.hash.as_str())),
    ]);
    let ceiling = spec
        .ceiling
        .unwrap_or(hh_provenance::AuthorityClass::Delegate)
        .min(
            ctx.handles
                .get(ctx.parent_handle_id)
                .map(|p| p.ceiling)
                .unwrap_or(hh_provenance::AuthorityClass::Delegate),
        );
    let attenuation_delta = Json::obj([
        (
            "requested_ceiling",
            Json::str(
                spec.ceiling
                    .unwrap_or(hh_provenance::AuthorityClass::Delegate)
                    .as_str(),
            ),
        ),
        ("granted_ceiling", Json::str(ceiling.as_str())),
        ("grants", Json::Int(spec.requested_grants.len() as i64)),
    ]);
    let child_handle_ids: Vec<String> = child_handles
        .iter()
        .map(|h| h.handle_id.as_str().to_string())
        .collect();
    let mut spawned_ev = kernel_ev_pub(
        ctx.store,
        ctx.parent_run_id,
        "control.subagent.spawned",
        spawned_payload(
            &child_run_id,
            &ctx.decision,
            &child_handle_ids,
            ctx.parent_handle_id,
            ceiling,
            &attenuation_delta,
            &child_budget_id,
            spec,
            env_handle_id.as_deref(),
            ctx.owner,
            ctx.parent_depth + 1,
            &parent_head,
            &reservation_id,
            spec.messaging_policy.as_ref(),
        ),
        vec![ctx.decision.clone()],
        None,
    )
    .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    // `control.subagent.spawned` opens the durable `child_run_id` scope. The
    // scope's open/close contract is class-owned; the spawn producer stamps
    // the only id it knows before the batch lands.
    spawned_ev.scope.child_run_id = Some(child_run_id.clone());
    // The `security.permission.granted{origin_basis: delegation}` audit row —
    // one per minted child handle (§5e.3 audit list).
    let mut batch = vec![spawned_ev];
    for h in &child_handles {
        batch.push(
            kernel_ev_pub(
                ctx.store,
                ctx.parent_run_id,
                "security.permission.granted",
                Json::obj([
                    ("handle_id", Json::str(h.handle_id.as_str())),
                    ("origin_basis", Json::str("delegation")),
                    ("holder", Json::str(child_run_id.as_str())),
                    ("ceiling", Json::str(h.ceiling.as_str())),
                    ("parent_handle", Json::str(ctx.parent_handle_id.as_str())),
                    (
                        "grants",
                        Json::Arr(
                            spec.requested_grants
                                .iter()
                                .map(|g| Json::str(g.effect.domain.name()))
                                .collect(),
                        ),
                    ),
                ]),
                vec![ctx.decision.clone()],
                None,
            )
            .map_err(|e| SpawnError::Kernel(e.to_string()))?,
        );
    }
    // The `budget.reserved` for spawns gets released against the spawned
    // row (the spawn consumed the slot — `spawns` stays counted; the
    // reservation is spent by charge? — spec: reserve holds the slot; the
    // spawned row IS the consumption. release frees the hold without
    // double-counting since `spawns` counts spawned rows in the fold).
    ctx.store
        .append(ctx.parent_run_id, ctx.parent_lease, batch)
        .map_err(|e| SpawnError::Kernel(format!("spawned append: {e}")))?;
    let spawn_event_id = ctx
        .store
        .head_event_id(ctx.parent_run_id)
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    // The spawned row is the batch head's parent — find it: it's the row
    // carrying child_run_id (the batch may have >1 events).
    let spawn_event = find_spawn_event(ctx.store, ctx.parent_run_id, &child_run_id)?;
    let _ = spawn_event_id;
    hook_phase!(ctx, SpawnPhase::AfterSpawnedAppend);

    // ── step 6 — child run creation + writer lease ───────────────────────
    let opened_lease = finish_child_open(ctx, spec, &child_run_id, &spawn_event, &parent_head)?;
    hook_phase!(ctx, SpawnPhase::AfterChildOpen);

    // ── step 7b — subscribe child_terminal on the parent ────────────────
    let subscription_id = ctx
        .store
        .wakeup_subscribe(
            ctx.parent_run_id,
            ctx.parent_lease,
            hh_ledger::wakeup::Trigger::ChildTerminal {
                child_run_id: child_run_id.clone(),
            },
            hh_ledger::wakeup::WakeupPolicy::default_policy(),
            &spawn_event,
        )
        .map_err(|e| SpawnError::Kernel(format!("child_terminal subscribe: {e}")))?;
    hook_phase!(ctx, SpawnPhase::AfterSubscribe);

    // The spawns reservation is released — the spawn consumed the slot
    // (release posts `released`; `spawns` accounting continues via the
    // spawned-row fold — INV-6's bound is the counter, not a held hold).
    {
        let mut account = Account::open(ctx.store, ctx.parent_run_id)
            .map_err(|e| SpawnError::Kernel(format!("account open: {e}")))?;
        let _ = account.release(ctx.parent_lease, &reservation_id);
    }

    // The child's writer lease — `open_run_with_id` acquired it under
    // `ctx.holder` at open; a pre-existing run means a takeover acquire.
    let child_lease = match opened_lease {
        Some(l) => l,
        None => ctx
            .store
            .acquire_writer(
                ctx.holder,
                &child_run_id,
                ctx.parent_lease
                    .expires_at_ms
                    .saturating_sub(ctx.store.now_ms()),
            )
            .map_err(|e| SpawnError::Kernel(format!("child writer lease: {e}")))?,
    };

    Ok(Spawned {
        child_run_id,
        child_handles: child_handle_ids,
        budget_id: child_budget_id,
        env_handle_id,
        spawn_event,
        child_lease: Some(child_lease),
        wait_mode: spec.wait_mode,
        subscription_id,
    })
}

/// Find the `control.subagent.spawned` event id for `child_run_id` (the
/// batch may carry sibling rows after it).
fn find_spawn_event(
    store: &Store,
    parent_run_id: &str,
    child_run_id: &str,
) -> Result<EventRef, SpawnError> {
    let events = store
        .events(parent_run_id)
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    for e in events.iter().rev() {
        if e.class == "control.subagent.spawned"
            && e.payload.get("child_run_id").and_then(Json::as_str) == Some(child_run_id)
        {
            return Ok(EventRef {
                run_id: parent_run_id.to_string(),
                event_id: e.event_id.clone(),
            });
        }
    }
    Err(SpawnError::Kernel(format!(
        "spawned row for {child_run_id} not found after append"
    )))
}

/// The `child_terminal{child}` subscription id on the parent (`None` when
/// the subscribing half never ran — the interrupted-spawn adoption path
/// creates it inside `finish_child_open`'s caller).
fn subscription_for(
    store: &Store,
    parent_run_id: &str,
    child_run_id: &str,
) -> Result<String, SpawnError> {
    let events = store
        .events(parent_run_id)
        .map_err(|e| SpawnError::Kernel(e.to_string()))?;
    for e in events.iter().rev() {
        if e.class != "control.wakeup.scheduled" {
            continue;
        }
        let sub = e.payload.get("subscription");
        let trig = sub.and_then(|s| s.get("trigger"));
        let is_child = matches!(
            trig.and_then(|t| t.get("type")).and_then(Json::as_str),
            Some("child_terminal")
        ) && trig
            .and_then(|t| t.get("child_run_id"))
            .and_then(Json::as_str)
            == Some(child_run_id);
        if is_child {
            if let Some(id) = sub
                .and_then(|s| s.get("subscription_id"))
                .and_then(Json::as_str)
            {
                return Ok(id.to_string());
            }
        }
    }
    Err(SpawnError::Kernel(format!(
        "no child_terminal subscription for {child_run_id}"
    )))
}

/// The child-run half (steps 6 + the deferred half of 7): `open_run_with_id`
/// under the deterministic id, manifest carrying `spawn_event` = the
/// parent's spawned row, `parent_run_id`, `activation_no = 1`, the parent's
/// `attendance`, `envelope_policy_ref`, and `extra.delegation_ref` +
/// `extra.parent_anchor` (ADR-0067 cross-run anchor). Runs on first spawn
/// and on the adopt-the-interrupted-spawn replay path.
fn finish_child_open(
    ctx: &mut SpawnCtx,
    spec: &SubagentSpec,
    child_run_id: &str,
    spawn_event: &EventRef,
    parent_head_at_spawn: &Json,
) -> Result<Option<Lease>, SpawnError> {
    if ctx.store.has_run(child_run_id) {
        return Ok(None);
    }
    let parent_manifest = ctx
        .store
        .manifest(ctx.parent_run_id)
        .map_err(|e| SpawnError::Kernel(e.to_string()))?
        .clone();
    let mut extra = BTreeMap::new();
    extra.insert(
        "delegation_ref".to_string(),
        Json::str(format!("{}:{}", ctx.decision.run_id, ctx.decision.event_id)),
    );
    // I-A6 reciprocal anchor: `parent_anchor` names the parent's run while
    // `seq`/`hash` are byte-equal to the parent row's
    // `parent_head_at_spawn` (the durable citation the spawn was admitted
    // against), not merely a later parent tip.
    extra.insert(
        "parent_anchor".to_string(),
        Json::obj([
            ("run_id", Json::str(ctx.parent_run_id)),
            (
                "seq",
                parent_head_at_spawn
                    .get("seq")
                    .cloned()
                    .unwrap_or(Json::Null),
            ),
            (
                "hash",
                parent_head_at_spawn
                    .get("hash")
                    .cloned()
                    .unwrap_or(Json::Null),
            ),
        ]),
    );
    extra.insert("subagent_spec".to_string(), spec.to_json());
    extra.insert("child_role".to_string(), Json::str(spec.role.as_str()));
    let harness_def_ref = match &spec.process {
        ChildProcess::Native { harness_def, .. } => match &harness_def.version {
            hh_hir::refs::RefVersion::Pinned(v) => Some(v.clone()),
            hh_hir::refs::RefVersion::Selector(_) => None,
        },
        ChildProcess::Hosted(_) => None,
    };
    let manifest = RunManifest {
        configuration_id: parent_manifest.configuration_id.clone(),
        configuration_version_id: parent_manifest.configuration_version_id.clone(),
        harness_def_ref: harness_def_ref.or_else(|| parent_manifest.harness_def_ref.clone()),
        model_profile_ref: parent_manifest.model_profile_ref.clone(),
        environment_ref: parent_manifest.environment_ref.clone(),
        environment_version_id: parent_manifest.environment_version_id.clone(),
        budget: None,
        seed: None,
        idp: "idp/1".to_string(),
        participant_class: parent_manifest.participant_class,
        observability_level: parent_manifest.observability_level.clone(),
        run_kind: hh_ledger::manifest::RunKind::Agent,
        activation_no: 1,
        attendance: parent_manifest.attendance,
        workspace_trust: parent_manifest.workspace_trust,
        hosting_mechanism: None,
        capability_declaration_ref: parent_manifest.capability_declaration_ref.clone(),
        parent_run_id: Some(ctx.parent_run_id.to_string()),
        spawn_event: Some(spawn_event.clone()),
        forked_from: None,
        continued_from: None,
        overrides_layer_id: parent_manifest.overrides_layer_id.clone(),
        envelope_policy_ref: parent_manifest.envelope_policy_ref.clone(),
        healing_policy_ref: parent_manifest.healing_policy_ref.clone(),
        lease_ttl: parent_manifest.lease_ttl,
        audit_policy_ref: parent_manifest.audit_policy_ref.clone(),
        // A spawned child participates in the same audit-policy family as
        // its parent: it may mint final checkpoints under the declared key
        // ids (custody stays outside the manifest).
        signer_key_ids: parent_manifest.signer_key_ids.clone(),
        grace_ms: parent_manifest.grace_ms,
        task_ref: None,
        experiment: None,
        registry_snapshot_id: parent_manifest.registry_snapshot_id.clone(),
        extra,
    };
    // `open_run_with_id` acquires the child's writer lease under `holder`
    // as part of the open — return it so `spawn` does not double-acquire.
    let (_id, lease) = ctx
        .store
        .open_run_with_id(child_run_id, manifest, ctx.holder)
        .map_err(|e| SpawnError::Kernel(format!("child open_run: {e}")))?;
    Ok(Some(lease))
}

/// `spawn`'s step-8 surface — what a `wait.mode = await` caller asks for:
/// the `delegation_completed` wait cue data (the caller suspends or polls
/// `wakeup_drain`; the cue itself is the parent's control-loop type — C3).
pub fn delegation_completed_cue(spawned: &Spawned) -> Json {
    Json::obj([
        ("kind", Json::str("wait")),
        ("wait", Json::str("delegation_completed")),
        ("child_run_id", Json::str(spawned.child_run_id.as_str())),
        (
            "subscription_id",
            Json::str(spawned.subscription_id.as_str()),
        ),
    ])
}
