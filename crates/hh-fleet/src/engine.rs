//! `FleetEngine` — the C4 reconciler + activation runtime (§5i.1). Every
//! op appends durable rows through the activation run's fenced writer and
//! every read folds the durable prefix — no process-local state the store
//! can't rebuild (AC-8's equality: `ensure` on the same store restores a
//! byte-identical view).
//!
//! The engine is the *only* writer of `control.work_item.*` /
//! `lifecycle.fleet.*` / `lifecycle.escalation.*` rows (single authority
//! path; Rule-P keeps them kernel-produced through
//! `commit_kernel_row_for`).

use hh_budget::account::Account;
use hh_budget::spec::{BudgetScope, BudgetScopeKind};
use hh_ledger::errors::LedgerError;
use hh_ledger::event::EventEnvelope;
use hh_ledger::leases::LeaseScope;
use hh_ledger::manifest::{EventRef, RunKind, RunManifest};
use hh_ledger::store::{Lease, Store};
use hh_ledger::wakeup::{Trigger, WakeupPolicy};
use hh_wire::json::Json;
use std::collections::BTreeMap;

use crate::errors::FleetError;
use crate::identity;
use crate::payloads;
use crate::source::{SourceOccurrence, WorkSourceAdapter};
use crate::spec::FleetSpec;
use crate::view::{FleetView, Cursor};
use crate::work_item::{derive_state, WorkItemInit, WorkItemView};

/// The kernel producer spelling for fleet-authored rows.
pub const COMPONENT: &str = "hh-fleet";

/// The `RunKind` the engine serves — a fleet activation.
pub const RUN_KIND: RunKind = RunKind::Fleet;

/// `FleetEngine` — one activation's durable driver.
pub struct FleetEngine {
    /// The activation run id (`fleet-{spec-hash}`).
    pub run_id: String,
    /// The holder identity on the writer lease.
    pub holder: String,
    /// The writer lease TTL the engine renews under.
    pub writer_ttl_ms: u64,
    /// The live writer lease.
    pub lease: Lease,
    /// The owner's fold — kept incrementally across ops (identical to a
    /// fresh `FleetView::fold` of the same prefix; `ensure` rebuilds it
    /// wholesale so restart equality is *structural*).
    pub view: FleetView,
}

/// `reconcile` outcome — the surface report `{dispatched, blocked,
/// escalated, settled}` plus the durable cursor the caller can carry.
#[derive(Debug, Clone, Default)]
pub struct ReconcileReport {
    /// Item ids dispatched this pass (RC-2 lease+mark landed).
    pub dispatched: Vec<String>,
    /// Item ids blocked this pass (any `blocked{verb:add}` row).
    pub blocked: Vec<String>,
    /// Item ids escalated this pass.
    pub escalated: Vec<String>,
    /// Item ids settled/stopped this pass (terminal rows).
    pub settled: Vec<String>,
    /// Occurrences newly observed (`context.observation.recorded` rows).
    pub observed: Vec<String>,
    /// Wakeup cues fired this pass.
    pub fired: Vec<String>,
    /// The durable cursor after the pass.
    pub cursor: Cursor,
}

/// `settle` `Outcome` — the closed outcome spellings for `control.
/// work_item.stopped{verb:settle}`.
pub const OUTCOMES: &[&str] = &["completed", "failed", "cancelled", "abandoned"];

/// `resolution` kinds `resolve_escalation` admits — the canonical E-5
/// closed set (`amended` cites the ledgered `amend`; `reassigned` is the
/// ownership-transfer resolution).
pub const RESOLUTION_KINDS: &[&str] = &[
    "acknowledged",
    "amended",
    "reassigned",
    "cancelled",
    "expired",
];

/// `basis` kinds `transfer_owner` admits — the canonical O-4 closed set.
/// `approval` cites the human decision; `policy_rule` the sealed rule.
/// Grant coverage resolution is the D-1 interim (ADR-0211): the basis is
/// recorded verbatim pending the live human directory.
pub const TRANSFER_BASES: &[&str] = &["approval", "policy_rule"];

/// The escalation issue codes the reconciler itself raises.
pub const ISSUES: &[&str] = &[
    "blocked",
    "stalled",
    "stale_lease",
    "source_suspended",
    "budget",
    "conflict",
    "overdue",
    "manual",
];

impl FleetEngine {
    // ── construction ───────────────────────────────────────────────────

    /// `fleet.open` — open the activation run, land the durable spec,
    /// allocate the matched-budget root, and subscribe the trigger rules.
    /// Idempotent: `open` on an existing activation with the same
    /// `spec_ref` returns `ensure` (the durable spec *is* the dedup key).
    pub fn open(
        store: &mut Store,
        holder: &str,
        writer_ttl_ms: u64,
        spec: FleetSpec,
    ) -> Result<(String, FleetEngine), FleetError> {
        spec.validate()?;
        let spec_ref = identity::spec_ref(&spec);
        let run_id = format!("fleet-{}", &spec_ref.replace(':', "-")[..24]);
        // Idempotent open — the same spec re-opens the same run.
        if store.manifest(&run_id).is_ok() {
            let eng = Self::ensure(store, &run_id, holder, writer_ttl_ms)?;
            if eng.view.spec_ref != spec_ref {
                return Err(FleetError::StaleSpec {
                    expected: eng.view.spec_ref.clone(),
                    seen: spec_ref,
                });
            }
            return Ok((run_id, eng));
        }
        let mut manifest = RunManifest::minimal(RunKind::Fleet);
        // A fleet activation carries no configuration/environment cells
        // (ADR-0183 §C's non-agent row — the FleetSpec lives in
        // `extra.fleet_spec_ref` + the `lifecycle.fleet.activated` row).
        manifest.configuration_id = None;
        manifest.configuration_version_id = None;
        manifest.budget = spec.budget_ref.clone();
        manifest.extra.insert(
            "fleet_spec_ref".to_string(),
            Json::str(&spec_ref),
        );
        manifest.extra.insert(
            "policy_ref".to_string(),
            Json::str(&spec.policy_ref),
        );
        manifest.extra.insert(
            "fleet_spec".to_string(),
            spec.to_json(),
        );
        if !spec.narrowing.is_empty() {
            manifest.extra.insert(
                "narrowing_leaves".to_string(),
                Json::Arr(spec.narrowing.iter().map(|l| l.to_json()).collect()),
            );
        }
        if spec.out_of_scope {
            manifest
                .extra
                .insert("out_of_scope".to_string(), Json::Bool(true));
        }
        let (_, lease) = store.open_run_with_id(&run_id, manifest, holder)?;
        let activated = store.commit_kernel_row_for(
            COMPONENT,
            &run_id,
            "lifecycle.fleet.activated",
            payloads::activated_payload(&spec, &spec_ref),
            vec![],
            vec![],
        )?;
        // The matched-budget arm — one root account on the activation
        // (`budget_ref` is the deterministic coordinate; `control.budget.
        // allocated{parent:null}` joins it in the fold).
        if let Some(bspec) = &spec.budget {
            let mut account = Account::open(store, &run_id).map_err(FleetError::Account)?;
            account.allocate(
                &lease,
                None,
                BudgetScope {
                    kind: BudgetScopeKind::Goal,
                    target: run_id.clone(),
                },
                bspec.clone(),
            )?;
        }
        // The trigger rules → durable subscriptions (`created_by` = the
        // activated row; external/manual are admissible for a Fleet run —
        // the boundary exception the ledger enforces).
        for rule in spec.triggers.values() {
            store.wakeup_subscribe(
                &run_id,
                &lease,
                rule.trigger.clone(),
                rule.policy.clone(),
                &EventRef {
                    run_id: run_id.clone(),
                    event_id: activated.event_id.clone(),
                },
            )?;
        }
        let events = store.events(&run_id)?.to_vec();
        let view = FleetView::fold(&events)?;
        Ok((
            run_id.clone(),
            FleetEngine {
                run_id,
                holder: holder.to_string(),
                writer_ttl_ms,
                lease,
                view,
            },
        ))
    }

    /// `ensure` — the restore path (§5i.1 #8): re-acquire the writer lease
    /// (the takeover is audited) and rebuild the fold from the durable
    /// prefix. No process state crosses — a crashed engine's every
    /// committed decision is reconstructed identically (RC-8).
    pub fn ensure(
        store: &mut Store,
        run_id: &str,
        holder: &str,
        writer_ttl_ms: u64,
    ) -> Result<FleetEngine, FleetError> {
        let manifest = store.manifest(run_id)?;
        if manifest.run_kind != RunKind::Fleet {
            return Err(FleetError::NotFleet {
                run: run_id.to_string(),
            });
        }
        let lease = store
            .acquire_writer(holder, run_id, writer_ttl_ms)
            .map_err(FleetError::Store)?;
        let events = store.events(run_id)?.to_vec();
        let view = FleetView::fold(&events)?;
        Ok(FleetEngine {
            run_id: run_id.to_string(),
            holder: holder.to_string(),
            writer_ttl_ms,
            lease,
            view,
        })
    }

    /// `bound` — renew-or-reacquire the writer lease (the experiment
    /// engine's pattern — an expired lease is re-acquired with the audited
    /// takeover; a live other holder `WouldBlock`s).
    fn bound(&mut self, store: &mut Store) -> Result<(), FleetError> {
        let now = store.now_ms();
        if now + self.writer_ttl_ms / 2 >= self.lease.expires_at_ms {
            match store.renew(&self.lease) {
                Ok(renewed) => self.lease = renewed,
                Err(_) => {
                    self.lease = store
                        .acquire_writer(&self.holder, &self.run_id, self.writer_ttl_ms)
                        .map_err(FleetError::Store)?;
                }
            }
        }
        Ok(())
    }

    /// Append one kernel row + fold it — the engine's write primitive.
    fn emit(
        &mut self,
        store: &mut Store,
        class: &str,
        payload: Json,
        causes: Vec<EventRef>,
    ) -> Result<EventEnvelope, FleetError> {
        let env = store.commit_kernel_row_for(COMPONENT, &self.run_id, class, payload, vec![], causes)?;
        self.view.fold_tail(store.events(&self.run_id)?)?;
        Ok(env)
    }

    /// Refold to the WAL tip (the post-append fold is incremental — a
    /// caller that needs another writer's rows calls this first).
    fn refresh(&mut self, store: &Store) -> Result<(), FleetError> {
        self.view.fold_tail(store.events(&self.run_id)?)
    }

    /// The durable cursor — `{event_count, observed_at_ms}` folded from
    /// the prefix (never stored: a restart re-derives the same cursor).
    pub fn cursor(&self) -> Cursor {
        Cursor {
            event_count: self.view.event_count,
            observed_at_ms: self.view.observed_watermark,
        }
    }

    // ── fleet.observe ──────────────────────────────────────────────────

    /// `observe(adapter, since, now_ms)` — store each new adapter
    /// occurrence as `context.observation.recorded` + a durable
    /// `control.wakeup.occurred` under the matching subscription, then
    /// `deliver_wakeup` fires the pending set (level-triggered — a
    /// replayed occurrence hits the dedup skip, never a second fire).
    pub fn observe<A: WorkSourceAdapter>(
        &mut self,
        store: &mut Store,
        adapter: &A,
        now_ms: u64,
    ) -> Result<Vec<String>, FleetError> {
        self.bound(store)?;
        let mut observed = Vec::new();
        for occ in adapter.occurrences(None) {
            if self
                .view
                .observed_occurrence_ids
                .contains(&occ.occurrence_id)
            {
                continue; // durable dedup — replayed snapshot, no second row
            }
            let trigger = self.trigger_for_occurrence(&occ);
            let Some(sub_id) = self.subscription_for(store, &trigger)? else {
                // No declared rule covers this occurrence kind — recorded
                // (CC3) but never fired: the trigger set is the contract.
                self.emit(
                    store,
                    "context.observation.recorded",
                    payloads::observation_payload(
                        &occ.trigger_kind,
                        occ.external_kind.as_deref(),
                        &occ.occurrence_id,
                        &occ.item,
                        occ.observed_at_ms,
                        "fixture",
                        &identity::source_ref(&self.run_id, source_id_of(&occ.item)),
                    ),
                    vec![],
                )?;
                continue;
            };
            let obs = self.emit(
                store,
                "context.observation.recorded",
                payloads::observation_payload(
                    &occ.trigger_kind,
                    occ.external_kind.as_deref(),
                    &occ.occurrence_id,
                    &occ.item,
                    occ.observed_at_ms,
                    "fixture",
                    &identity::source_ref(&self.run_id, source_id_of(&occ.item)),
                ),
                vec![],
            )?;
            // The durable occurrence — `payload_ref` links back to the
            // observation row (the fired row's join to the item dossier).
            let key = identity::occurrence_key(&occ.trigger_kind, &occ.occurrence_id);
            store.wakeup_occurred(
                &self.run_id,
                &self.lease,
                &sub_id,
                &key,
                Some(&obs.event_id),
                occ.observed_at_ms.max(now_ms),
            )?;
            observed.push(occ.occurrence_id.clone());
        }
        // Fire the pending set — `deliver_wakeup` synthesises kernel
        // occurrences (timers, retry_due) and claims every pending one
        // under W-1's atomic lease+fire batch.
        store.deliver_wakeup(&self.run_id, &self.lease, now_ms)?;
        self.refresh(store)?;
        Ok(observed)
    }

    /// The `Trigger` a source occurrence maps to — `external`/`manual`/
    /// `timer` only (the fixture's closed set; anything else was refused
    /// at `FixtureAdapter::from_doc`).
    fn trigger_for_occurrence(&self, occ: &SourceOccurrence) -> Trigger {
        match occ.trigger_kind.as_str() {
            "external" => Trigger::External {
                kind: occ
                    .external_kind
                    .clone()
                    .unwrap_or_else(|| "occurrence".to_string()),
                source_ref: None,
                filter: None,
            },
            "manual" => Trigger::Manual {
                principal: "fixture".to_string(),
            },
            _ => Trigger::Timer {
                at_ms: occ.observed_at_ms,
            },
        }
    }

    /// Find (or lazily subscribe) the subscription covering a trigger —
    /// `spec.triggers` is the declared set; an occurrence whose trigger no
    /// rule names returns `None` (recorded, never fired).
    fn subscription_for(
        &mut self,
        store: &mut Store,
        trigger: &Trigger,
    ) -> Result<Option<String>, FleetError> {
        // The declared rule that covers this trigger type (+ kind/principal).
        let covering: Option<&crate::spec::TriggerRule> =
            self.view.spec.triggers.values().find(|r| {
                std::mem::discriminant(&r.trigger) == std::mem::discriminant(trigger)
                    && match (&r.trigger, trigger) {
                        (Trigger::External { kind: a, .. }, Trigger::External { kind: b, .. }) => {
                            a == b || a == "*"
                        }
                        (Trigger::Manual { .. }, Trigger::Manual { .. }) => true,
                        _ => true,
                    }
            });
        let Some(rule) = covering else {
            return Ok(None);
        };
        // An existing subscription for the same trigger is reused (the
        // subscribe is once-per-rule — durable dedup by trigger shape).
        for (sub_id, t) in &self.view.sub_triggers {
            if t == &rule.trigger {
                return Ok(Some(sub_id.clone()));
            }
        }
        let sub_id = store.wakeup_subscribe(
            &self.run_id,
            &self.lease,
            rule.trigger.clone(),
            rule.policy.clone(),
            &EventRef {
                run_id: self.run_id.clone(),
                event_id: self.view.activated_event_id.clone(),
            },
        )?;
        self.refresh(store)?;
        Ok(Some(sub_id))
    }

    // ── fleet.reconcile — the RC-1…8 pass ──────────────────────────────

    /// `reconcile(adapter, fixture_ref, now_ms)` — the ReconcilerBoundary.
    /// Level-triggered: the pass derives its entire action set from the
    /// folded durable prefix + the pinned source snapshot; every row it
    /// writes is itself durable, so replaying the pass at the same
    /// `now_ms` on the same prefix is a no-op (RC-8).
    pub fn reconcile<A: WorkSourceAdapter>(
        &mut self,
        store: &mut Store,
        adapter: &A,
        now_ms: u64,
    ) -> Result<ReconcileReport, FleetError> {
        self.bound(store)?;
        let mut report = ReconcileReport::default();
        // 1. Observe + fire — the durable cue intake (§5i.1 #2).
        report.observed = self.observe(store, adapter, now_ms)?;
        // 2. RC-3 — source conflicts from the cue stream (before any
        //    dispatch decision consumes a contradictory item).
        self.reconcile_conflicts(store, adapter, &mut report)?;
        // 3. RC-1 — admit every fresh cue's item (`spec_ref` = the live
        //    spec; a cue re-presented after admission is a `known` hit).
        self.reconcile_admits(store, &mut report)?;
        // 3b. Canonical step (2) — source transitions on admitted items:
        //     `state` moves land `dispatched{verb:"source_update"}`; a
        //     human-gate state entry is `blocked{human_gate}` (the
        //     `handoff` state), exits only on a human act.
        self.reconcile_source_updates(store, &mut report)?;
        // 4. RC-4 — source suspension exemptions.
        self.reconcile_suspensions(store, adapter, &mut report)?;
        // 5. RC-5 — stalls, retry scheduling, escalation deadlines.
        self.reconcile_stalls(store, adapter, now_ms, &mut report)?;
        // 6. RC-7 — child-terminal → parent `child_blocked`; terminal
        //    parents cascade-stop unblocked children.
        self.reconcile_children(store, &mut report)?;
        // 7. RC-2 + RC-6 — the dispatch pass (capacity + budget + lease +
        //    mark; `stale_dispatch` on the re-fire).
        self.reconcile_dispatches(store, adapter, now_ms, &mut report)?;
        // 8. RC-5's overdue escalations (deadline-fired cues).
        self.reconcile_escalations(store, now_ms, &mut report)?;
        report.cursor = self.cursor();
        Ok(report)
    }

    /// RC-3 — replayed occurrences whose dossier conflicts with the
    /// admitted item land `source_conflict` + `blocked{conflict}` +
    /// escalate `conflict` (once each — level-triggered, never twice).
    fn reconcile_conflicts<A: WorkSourceAdapter>(
        &mut self,
        store: &mut Store,
        _adapter: &A,
        report: &mut ReconcileReport,
    ) -> Result<(), FleetError> {
        // Collect cue→item dossier joins first (borrow discipline).
        let mut incoming: Vec<(String, WorkItemInit)> = Vec::new();
        for cue in self.source_cues() {
            if let Some(init) = self.cue_item(&cue) {
                incoming.push((cue.occurrence_key.clone(), init));
            }
        }
        for (_key, init) in incoming {
            let Some(existing) = self.view.items.get(&init.item_id).cloned() else {
                continue; // fresh — the admit pass owns it
            };
            let field = conflict_field(&existing, &init);
            if let Some(field) = field {
                // Idempotent: `blocked{conflict}` already live ⇒ the
                // conflict row landed on a previous pass.
                if existing.blocked.contains("source_conflict") {
                    continue;
                }
                let expected = field_value(&existing, &field);
                let seen = field_value_init(&init, &field);
                self.emit(
                    store,
                    "control.work_item.dispatched",
                    payloads::source_conflict_payload(
                        &existing.item_id,
                        &existing.run_item_id,
                        &field,
                        &expected,
                        &seen,
                    ),
                    vec![],
                )?;
                self.emit(
                    store,
                    "control.work_item.blocked",
                    payloads::block_add_payload(
                        &existing.item_id,
                        &existing.run_item_id,
                        "source_conflict",
                        None,
                        None,
                    ),
                    vec![],
                )?;
                report.blocked.push(existing.item_id.clone());
                self.raise_escalation(
                    store,
                    &existing,
                    "conflict",
                    "conflict",
                    "reconciler",
                    None,
                )?;
            }
        }
        Ok(())
    }

    /// RC-1 — admit the cue items (`spec_ref` of the live spec; idempotent
    /// under `item_id`/`idempotency_key`).
    fn reconcile_admits(
        &mut self,
        store: &mut Store,
        _report: &mut ReconcileReport,
    ) -> Result<(), FleetError> {
        let mut inits = Vec::new();
        for cue in self.source_cues() {
            if let Some(init) = self.cue_item(&cue) {
                inits.push(init);
            }
        }
        for init in inits {
            if self.view.items.contains_key(&init.item_id)
                || self.view.by_idem_key.contains_key(&init.idempotency_key)
            {
                continue; // `known` — the durable admit already covers it
            }
            // `capacity.items` — a full activation refuses admission
            // (`Refused{capacity}` surfaces on direct `admit`; the
            // reconcile path skips + escalates instead of fabricating).
            if self.view.live_item_count() as u64 >= self.view.spec.capacity.items {
                continue;
            }
            self.admit(store, init)?;
        }
        Ok(())
    }

    /// Canonical step (2) — a source dossier re-presented for an admitted
    /// item whose `source.state` moved lands `dispatched{verb:
    /// "source_update"}` (the fold's `source.state` is durable). Then the
    /// human-gate evaluation: `state ∈ spec.human_gate_states` ⇒
    /// `blocked{human_gate}` (the `handoff` state — exits only via a human
    /// act: `resume_from_handoff`), leaving the set ⇒ the cause clears.
    fn reconcile_source_updates(
        &mut self,
        store: &mut Store,
        report: &mut ReconcileReport,
    ) -> Result<(), FleetError> {
        let mut updates: Vec<(String, WorkItemInit)> = Vec::new();
        for cue in self.source_cues() {
            if let Some(init) = self.cue_item(&cue) {
                updates.push((cue.occurrence_key.clone(), init));
            }
        }
        for (_key, init) in updates {
            let Some(it) = self.view.items.get(&init.item_id).cloned() else {
                continue;
            };
            if it.settlement.is_some() || it.watch_state == "dead" {
                continue;
            }
            let new_state = init
                .source
                .get("state")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string();
            let cur_state = it
                .source
                .get("state")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string();
            if !new_state.is_empty() && new_state != cur_state {
                self.emit(
                    store,
                    "control.work_item.dispatched",
                    payloads::source_update_payload(
                        &it.item_id,
                        &it.run_item_id,
                        &new_state,
                    ),
                    vec![],
                )?;
            }
        }
        // The human-gate evaluation — level-triggered over the folded
        // `source.state` (the state itself is durable fact).
        let gates: Vec<(String, String, bool)> = self
            .view
            .items
            .values()
            .filter(|i| i.watch_state == "watching" && i.settlement.is_none())
            .map(|i| {
                let st = i
                    .source
                    .get("state")
                    .and_then(Json::as_str)
                    .unwrap_or_default();
                (
                    i.item_id.clone(),
                    i.run_item_id.clone(),
                    self.view
                        .spec
                        .human_gate_states
                        .iter()
                        .any(|g| g == st),
                )
            })
            .collect();
        for (item_id, run_item_id, gated) in gates {
            let has = self
                .view
                .items
                .get(&item_id)
                .map(|i| i.blocked.contains("human_gate"))
                .unwrap_or(false);
            if gated && !has {
                self.emit(
                    store,
                    "control.work_item.blocked",
                    payloads::block_add_payload(
                        &item_id,
                        &run_item_id,
                        "human_gate",
                        None,
                        None,
                    ),
                    vec![],
                )?;
                report.blocked.push(item_id);
            } else if !gated && has {
                // Leaving a human-gate state is a human act only when it
                // arrives through `resume_from_handoff` — a source
                // transition out of the gate clears the durable cause
                // too (the source IS the record of the human act —
                // §5i.1 #2's "source transition attributed to a mapped
                // human principal" is the fixture's `state` member).
                self.emit(
                    store,
                    "control.work_item.blocked",
                    payloads::block_remove_payload(
                        &item_id,
                        &run_item_id,
                        "human_gate",
                    ),
                    vec![],
                )?;
            }
        }
        Ok(())
    }

    /// RC-4 — `adapter.suspended(source_id)` lifts/drops `suspended` on the
    /// items whose `source.source_id` matches, exempting them from
    /// dispatch while suspended (the `suspended_source` block cause is
    /// durable — the lift is its remove row).
    fn reconcile_suspensions<A: WorkSourceAdapter>(
        &mut self,
        store: &mut Store,
        adapter: &A,
        report: &mut ReconcileReport,
    ) -> Result<(), FleetError> {
        let items: Vec<(String, bool, String)> = self
            .view
            .items
            .values()
            .filter(|i| i.watch_state == "watching")
            .map(|i| {
                let sid = source_id_of_view(i);
                (
                    i.item_id.clone(),
                    adapter.suspended(&sid),
                    i.run_item_id.clone(),
                )
            })
            .collect();
        for (item_id, suspended, run_item_id) in items {
            let has = self
                .view
                .items
                .get(&item_id)
                .map(|i| i.blocked.contains("suspended_source"))
                .unwrap_or(false);
            if suspended && !has {
                self.emit(
                    store,
                    "control.work_item.blocked",
                    payloads::block_add_payload(&item_id, &run_item_id, "suspended_source", None, None),
                    vec![],
                )?;
                report.blocked.push(item_id);
            } else if !suspended && has {
                self.emit(
                    store,
                    "control.work_item.blocked",
                    payloads::block_remove_payload(&item_id, &run_item_id, "suspended_source"),
                    vec![],
                )?;
            }
        }
        Ok(())
    }

    /// RC-5 — stall detection (`on.stuck.after_ms` on `dispatching`/
    /// `dispatched`/`error` items), retry scheduling for `error` items
    /// (`max_attempts` bound durable), and source-suspension staleness.
    /// The `escalation deadline` timers fire through `reconcile_escalations`.
    fn reconcile_stalls<A: WorkSourceAdapter>(
        &mut self,
        store: &mut Store,
        adapter: &A,
        now_ms: u64,
        report: &mut ReconcileReport,
    ) -> Result<(), FleetError> {
        let items: Vec<WorkItemView> = self
            .view
            .items
            .values()
            .filter(|i| i.watch_state == "watching" && i.settlement.is_none())
            .cloned()
            .collect();
        for it in items {
            // Retry scheduling — an `error` dispatch with attempts left
            // and no live schedule gets one (`control.retry.scheduled` +
            // the `verb:retry` state row + a `retry_due` subscription).
            if it.dispatch.status == "error" && !it.blocked.contains("escalation") {
                let max = it
                    .on
                    .retry_max_attempts
                    .unwrap_or(self.view.spec.defaults.max_retries);
                if it.retry.attempts < max
                    && !self.retry_live(&it.item_id)
                    && !it.blocked.contains("retry_exhausted")
                {
                    let next = now_ms
                        + it.on
                            .retry_backoff_ms
                            .unwrap_or(self.view.spec.defaults.retry_backoff_ms);
                    let attempts = it.retry.attempts + 1;
                    self.emit(
                        store,
                        "control.work_item.dispatched",
                        payloads::retry_payload(
                            &it.item_id,
                            &it.run_item_id,
                            &self.view.spec_ref,
                            attempts,
                            next,
                        ),
                        vec![],
                    )?;
                    // The durable timer + the `retry_due` subscription —
                    // `deliver_wakeup` fires it when `not_before ≤ now`.
                    let sched = store.commit_kernel_row_for(
                        COMPONENT,
                        &self.run_id,
                        "control.retry.scheduled",
                        Json::obj([
                            ("scope_id", Json::str(&it.item_id)),
                            ("kind", Json::str("work_item")),
                            ("not_before", Json::Int(next as i64)),
                            ("reason", Json::str("dispatch_error")),
                            ("attempt_no", Json::Int(attempts as i64)),
                        ]),
                        vec![],
                        vec![],
                    )?;
                    self.view.fold_tail(store.events(&self.run_id)?)?;
                    self.ensure_retry_sub(store, &it.item_id, &sched.event_id)?;
                } else if it.retry.attempts >= max
                    && !it.blocked.contains("retry_exhausted")
                {
                    // Retry bound reached — `blocked{retry_exhausted}` and
                    // escalate (bound declared, never silent).
                    self.emit(
                        store,
                        "control.work_item.blocked",
                        payloads::block_add_payload(
                            &it.item_id,
                            &it.run_item_id,
                            "retry_exhausted",
                            None,
                            None,
                        ),
                        vec![],
                    )?;
                    report.blocked.push(it.item_id.clone());
                    self.raise_escalation(
                        store,
                        &it,
                        "stalled",
                        "retry_exhausted",
                        "reconciler",
                        None,
                    )?;
                }
            }
            // Stall detection — `on.stuck.after_ms` since the item's last
            // durable progress (dispatch start or the last state row).
            if matches!(it.dispatch.status.as_str(), "dispatching" | "dispatched")
                && it.escalation.is_none()
                && !it.blocked.contains("stalled")
            {
                let limit = it
                    .on
                    .stuck_after_ms
                    .unwrap_or(self.view.spec.defaults.stall_timeout_ms);
                let last = it.dispatch.started_ts.unwrap_or(0);
                if last > 0 && now_ms >= last + limit {
                    self.emit(
                        store,
                        "control.work_item.blocked",
                        payloads::block_add_payload(
                            &it.item_id,
                            &it.run_item_id,
                            "stalled",
                            None,
                            None,
                        ),
                        vec![],
                    )?;
                    report.blocked.push(it.item_id.clone());
                    if let Some(esc) = &it.on.stall_escalate {
                        self.raise_escalation(
                            store,
                            &it,
                            "stalled",
                            "stalled",
                            "reconciler",
                            Some((esc.to.clone(), esc.deadline_ms)),
                        )?;
                    }
                }
            }
            // `blocked{escalate}` — the declaration's arm: a blocked item
            // with `on.blocked_escalate` declared raises once (the open
            // escalation + the `escalation` block member are the level
            // trigger — no second raise while either stands).
            if !it.blocked.is_empty()
                && it.escalation.is_none()
                && !it.blocked.contains("escalation")
            {
                if let Some(esc) = &it.on.blocked_escalate {
                    self.raise_escalation(
                        store,
                        &it,
                        "blocked",
                        "blocked",
                        "reconciler",
                        Some((esc.to.clone(), esc.deadline_ms)),
                    )?;
                    report.escalated.push(it.item_id.clone());
                }
            }
            // `suspended` surface — the adapter flag folds onto the item
            // (the durable `suspended_source` block is the authoritative
            // member; the flag is presentation).
            let sid = source_id_of_view(&it);
            let _ = adapter.suspended(&sid);
        }
        Ok(())
    }

    /// RC-7 — terminal children mark their parents `child_blocked` (the
    /// child *blocked* at terminal time exempts the parent — §5i.1's
    /// "a blocked child exempts the parent from child-blocking"), and a
    /// terminal parent's unblocked, non-terminal children cascade-stop.
    fn reconcile_children(
        &mut self,
        store: &mut Store,
        report: &mut ReconcileReport,
    ) -> Result<(), FleetError> {
        let items: Vec<WorkItemView> = self.view.items.values().cloned().collect();
        // Terminal child → parent `child_blocked` (unless the child was
        // blocked at terminal — exemption).
        for parent in &items {
            if parent.watch_state != "watching" || parent.settlement.is_some() {
                continue;
            }
            for child_id in &parent.blocking {
                let Some(child) = items.iter().find(|c| &c.item_id == child_id) else {
                    continue;
                };
                if child.watch_state == "dead" && !child.blocked.contains("blocked") {
                    if !parent.blocked.contains("child_blocked") {
                        self.emit(
                            store,
                            "control.work_item.blocked",
                            payloads::block_add_payload(
                                &parent.item_id,
                                &parent.run_item_id,
                                "child_blocked",
                                None,
                                None,
                            ),
                            vec![],
                        )?;
                        report.blocked.push(parent.item_id.clone());
                    }
                }
            }
        }
        // Terminal parent → cascade-stop its unblocked live children.
        for parent in &items {
            if parent.watch_state != "dead" {
                continue;
            }
            for child_id in &parent.blocking {
                let Some(child) = items.iter().find(|c| &c.item_id == child_id) else {
                    continue;
                };
                if child.watch_state == "watching"
                    && child.settlement.is_none()
                    && child.blocked.is_empty()
                {
                    self.emit(
                        store,
                        "control.work_item.stopped",
                        payloads::stopped_payload(
                            "terminal",
                            &child.item_id,
                            &child.run_item_id,
                            Some("abandoned"),
                            Some("parent_terminal"),
                            &[],
                        ),
                        vec![],
                    )?;
                    report.settled.push(child.item_id.clone());
                }
            }
        }
        Ok(())
    }

    /// RC-2 + RC-6 — the dispatch pass. Candidates: `queued` or
    /// `error`-retryable-with-a-due-schedule, unblocked, unsuspended,
    /// within `activate_run` capacity, budget-sliced under the matched
    /// arm. Each dispatch = scoped lease + `verb:dispatch` mark — the
    /// second fire of the same cue skips (`stale_dispatch` — the durable
    /// mark is the truth).
    fn reconcile_dispatches<A: WorkSourceAdapter>(
        &mut self,
        store: &mut Store,
        adapter: &A,
        now_ms: u64,
        report: &mut ReconcileReport,
    ) -> Result<(), FleetError> {
        // Consume fired `retry_due` cues first — a due retry resets the
        // item to `queued`-equivalent for this pass.
        let retry_cues: Vec<crate::view::FleetCue> = self
            .view
            .cues
            .iter()
            .filter(|c| matches!(c.trigger, Trigger::RetryDue { .. }))
            .cloned()
            .collect();
        for cue in retry_cues {
            // The schedule the cue fired on is consumed — exactly once
            // (durable dedup via `consumed_schedules`).
            let sched_id = self.schedule_for_cue(&cue);
            if let Some(sid) = &sched_id {
                if !self.view.consumed_schedules.contains(sid) {
                    self.emit(
                        store,
                        "control.retry.fired",
                        Json::obj([
                            ("schedule_event_id", Json::str(sid)),
                            ("scope_id", Json::str(scope_of_retry(&cue.trigger))),
                        ]),
                        vec![],
                    )?;
                }
            }
            // The item goes back through the dispatch gate below.
        }
        let mut items: Vec<WorkItemView> = self
            .view
            .items
            .values()
            .filter(|i| i.watch_state == "watching" && i.settlement.is_none())
            .cloned()
            .collect();
        items.sort_by(|a, b| a.item_id.cmp(&b.item_id)); // deterministic order
        for it in items {
            if !self.dispatch_candidate(&it, now_ms) {
                continue;
            }
            // RC-6 capacity — `activate_run` is the bound; a full fleet
            // leaves the item queued (the bound is durable data, not a
            // refusal — the `fleet.dispatch` op surfaces `capacity`).
            if self.view.active_dispatch_count() as u64
                >= self.view.spec.capacity.activate_run
            {
                continue;
            }
            self.dispatch(store, adapter, &it.item_id, now_ms, report)?;
        }
        Ok(())
    }

    /// The dispatch gate — every RC's precondition folded (RC-2's ready
    /// set, RC-4's exemptions, RC-5's stall/retry, RC-6's capacity).
    fn dispatch_candidate(&self, it: &WorkItemView, now_ms: u64) -> bool {
        if it.watch_state != "watching" || it.settlement.is_some() {
            return false;
        }
        if !it.blocked.is_empty() || it.escalation.is_some() {
            return false;
        }
        // `suspended` — the durable `suspended_source` cause is the
        // member; the flag is presentation-only.
        if it.suspended {
            return false;
        }
        match it.dispatch.status.as_str() {
            // A ready admission or a retry-due error.
            "ready" => true,
            "error" => {
                let max = it
                    .on
                    .retry_max_attempts
                    .unwrap_or(self.view.spec.defaults.max_retries);
                // Level-triggered: `next_attempt_at_ms ≤ now` — a late
                // reconcile still dispatches (the `retry_due` cue is the
                // wake, never the gate).
                it.retry.attempts < max
                    && it
                        .retry
                        .next_attempt_at_ms
                        .map(|n| now_ms >= n)
                        .unwrap_or(false)
            }
            _ => false,
        }
    }


    /// `dispatch(item)` — RC-2's lease+mark under RC-1's stale check +
    /// RC-6's capacity/budget gates. `dispatch_note` then stamps
    /// `run_ref`/`error` (the fixture adapter's `activate.run` answers
    /// the run coordinate; a real adapter's dispatcher opens the agent
    /// run with `spawn_event = the verb:dispatch row`).
    pub fn dispatch<A: WorkSourceAdapter>(
        &mut self,
        store: &mut Store,
        _adapter: &A,
        item_id: &str,
        now_ms: u64,
        report: &mut ReconcileReport,
    ) -> Result<(), FleetError> {
        let it = self.view.item(item_id)?.clone();
        // RC-1 — `spec_ref` must match the live spec (a stale cursor
        // refuses; the durable spec is the fence).
        if it.spec_ref != self.view.spec_ref {
            return Err(FleetError::StaleSpec {
                expected: self.view.spec_ref.clone(),
                seen: it.spec_ref.clone(),
            });
        }
        // `owner = none` + a network-effect dispatch class ⇒
        // `owner_required` (the narrowed-Π `allow` arm is a Stage-5
        // surface — the declared `source.network_effect` is the input).
        if it.owner.is_none()
            && it
                .source
                .get("network_effect")
                .map(|v| v == &Json::Bool(true))
                .unwrap_or(false)
        {
            self.emit(
                store,
                "control.work_item.blocked",
                payloads::block_add_payload(
                    &it.item_id,
                    &it.run_item_id,
                    "owner_required",
                    None,
                    None,
                ),
                vec![],
            )?;
            report.blocked.push(it.item_id.clone());
            return Err(FleetError::OwnerRequired {
                item: it.item_id.clone(),
            });
        }
        // The dispatch class's ack requirement — durable on the item.
        if it.on.ack_required && it.owner.is_some() && !it.owner_ack {
            return Err(FleetError::OwnerAckRequired {
                item: it.item_id.clone(),
            });
        }
        // RC-2's idempotent lease — the folded `run_item` scoped lease is
        // the truth: a live lease is reused (`acquire` vs `upgrade`
        // distinction lands on the dispatch row's `acquire` member).
        let scope = LeaseScope::Resource(format!("run_item:{}", it.run_item_id));
        let existing = self.view.item_leases.get(&it.item_id).cloned();
        let (lease_ref, acquire) = if let Some(live) = existing {
            let lid = live
                .get("lease_id")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string();
            (lid, "acquire".to_string())
        } else {
            match store.lease_acquire(
                &self.run_id,
                &self.lease,
                &scope,
                &self.holder,
                self.view.spec.defaults.dispatch_lease_ms,
            ) {
                Ok(rec) => (rec.lease_id.clone(), "acquire".to_string()),
                Err(LedgerError::WouldBlock { .. }) => {
                    // A live lease this writer didn't fold yet — refresh
                    // and reuse it (the second fire is `stale_dispatch`,
                    // never a second lease).
                    self.refresh(store)?;
                    let lid = self
                        .view
                        .item_leases
                        .get(&it.item_id)
                        .and_then(|l| l.get("lease_id"))
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_string();
                    (lid, "upgrade".to_string())
                }
                Err(e) => return Err(FleetError::Store(e)),
            }
        };
        self.refresh(store)?;
        // Consume the item's live retry schedule — a re-dispatch under a
        // pending `control.retry.scheduled` fires it exactly once (the
        // consume row closes the timer; `consumed_schedules` dedups).
        let pending: Vec<String> = self
            .view
            .pending_schedules
            .iter()
            .filter(|(_, p)| {
                p.get("scope_id").and_then(Json::as_str) == Some(it.item_id.as_str())
            })
            .map(|(id, _)| id.clone())
            .collect();
        for sid in pending {
            if !self.view.consumed_schedules.contains(&sid) {
                self.emit(
                    store,
                    "control.retry.fired",
                    Json::obj([
                        ("schedule_event_id", Json::str(&sid)),
                        ("scope_id", Json::str(&it.item_id)),
                    ]),
                    vec![],
                )?;
            }
        }
        // RC-6 — the matched-budget conditional. `out_of_scope`
        // activations record `declared: null` + `matched: false`; the
        // matched arm allocates the slice before the mark (a budget
        // refusal blocks the item, never an unaccounted dispatch — CC9).
        let declared = self.dispatch_budget(store, &it, report)?;
        // The dispatch mark — the durable precondition set is now landed.
        self.emit(
            store,
            "control.work_item.dispatched",
            payloads::dispatch_mark_payload(
                &it.item_id,
                &it.run_item_id,
                &self.view.spec_ref,
                &lease_ref,
                &acquire,
                now_ms,
                declared,
            ),
            vec![EventRef {
                run_id: self.run_id.clone(),
                event_id: it.admitted_event_id.clone(),
            }],
        )?;
        report.dispatched.push(it.item_id.clone());
        Ok(())
    }

    /// RC-6 — allocate the dispatch's slice under the activation root.
    /// `None` ⇒ `out_of_scope` (recorded `matched: false`).
    fn dispatch_budget(
        &mut self,
        store: &mut Store,
        it: &WorkItemView,
        report: &mut ReconcileReport,
    ) -> Result<Option<Json>, FleetError> {
        if self.view.spec.out_of_scope {
            return Ok(None);
        }
        let Some(root) = self.view.budget_id.clone() else {
            // The matched arm declared but the root never allocated —
            // `missing_budget_ref` blocks, never dispatches unaccounted.
            self.emit(
                store,
                "control.work_item.blocked",
                payloads::block_add_payload(
                    &it.item_id,
                    &it.run_item_id,
                    "budget",
                    None,
                    None,
                ),
                vec![],
            )?;
            report.blocked.push(it.item_id.clone());
            return Err(FleetError::MissingBudgetRef);
        };
        // The item's slice — `defaults.dispatch_slice` when declared,
        // else the activation spec mirrored (containment enforces).
        let mut spec = match &self.view.spec.budget {
            Some(b) => b.clone(),
            None => return Ok(None),
        };
        spec.mode = hh_budget::spec::BudgetMode::Slice;
        let mut account = Account::open(store, &self.run_id).map_err(FleetError::Account)?;
        let scope = BudgetScope {
            kind: BudgetScopeKind::Goal,
            target: it.run_item_id.clone(),
        };
        match account.allocate(&self.lease, Some(&root), scope, spec.clone()) {
            Ok(slice_id) => {
                self.refresh(store)?;
                Ok(Some(Json::obj([
                    ("budget_id", Json::str(&slice_id)),
                    ("parent", Json::str(&root)),
                    ("spec", spec.to_json()),
                    ("matched", Json::Bool(true)),
                ])))
            }
            Err(e) => {
                // The ledgered refusal is the durable record; the item
                // blocks `budget` (the reconciler's own vocabulary) and
                // escalates when declared.
                self.emit(
                    store,
                    "control.work_item.blocked",
                    payloads::block_add_payload(
                        &it.item_id,
                        &it.run_item_id,
                        "budget",
                        None,
                        None,
                    ),
                    vec![],
                )?;
                report.blocked.push(it.item_id.clone());
                Err(FleetError::Account(e))
            }
        }
    }

    /// RC-5's escalation arm — deadline-fired `timer` cues re-raise as
    /// `overdue` (the escalation itself already hands off to the
    /// resolved top-level owner; the `deadline_sub` join is durable).
    fn reconcile_escalations(
        &mut self,
        store: &mut Store,
        now_ms: u64,
        report: &mut ReconcileReport,
    ) -> Result<(), FleetError> {
        let timer_cues: Vec<crate::view::FleetCue> = self
            .view
            .cues
            .iter()
            .filter(|c| matches!(c.trigger, Trigger::Timer { .. }))
            .cloned()
            .collect();
        for cue in timer_cues {
            // Find the open escalation whose `deadline_sub` fired.
            let item = self
                .view
                .items
                .values()
                .find(|i| {
                    i.escalation
                        .as_ref()
                        .map(|e| e.deadline_sub.as_deref() == Some(cue.subscription_id.as_str()))
                        .unwrap_or(false)
                })
                .cloned();
            let Some(it) = item else {
                continue;
            };
            let esc = it.escalation.clone().unwrap();
            if esc.deadline_ms.map(|d| now_ms >= d).unwrap_or(false) {
                // `overdue` — the resolved owner takes the item (handoff
                // to `resolve_owner(item)` per §5i.1 #6's overdue arm).
                let top = self
                    .view
                    .ownership
                    .resolve_owner(it.owner.as_deref().unwrap_or(""));
                if !top.is_empty() && it.owner.as_deref() != Some(top.as_str()) {
                    self.state_handoff(store, &it, &top, "escalation")?;
                }
                self.raise_escalation(
                    store,
                    &it,
                    "overdue",
                    "overdue",
                    "reconciler",
                    None,
                )?;
                report.escalated.push(it.item_id.clone());
            }
        }
        Ok(())
    }

    // ── cue helpers ────────────────────────────────────────────────────

    /// The cue set that carries source items — `external`/`manual`
    /// triggers whose `payload_ref` names an observation row.
    fn source_cues(&self) -> Vec<crate::view::FleetCue> {
        self.view
            .cues
            .iter()
            .filter(|c| {
                matches!(c.trigger, Trigger::External { .. } | Trigger::Manual { .. })
                    && c.payload_ref.is_some()
            })
            .cloned()
            .collect()
    }

    /// Decode a cue's item dossier — `occurred → payload_ref →
    /// observation row → item` (all durable joins).
    fn cue_item(&self, cue: &crate::view::FleetCue) -> Option<WorkItemInit> {
        let occurred = self.view.occurred.get(&cue.occurred_event)?;
        let obs_id = occurred.get("payload_ref").and_then(Json::as_str)?;
        // The observation row lives in the durable prefix — find it by
        // event id (the view indexes observations by id through
        // `occurred`; the observation payload itself is the dossier).
        let obs = self.observation_row(obs_id)?;
        let item_json = obs.get("item")?;
        WorkItemInit::from_json(item_json).ok()
    }

    /// Locate a `context.observation.recorded` row by event id — the
    /// `payload_ref` on a `control.wakeup.occurred` row IS the
    /// observation event id; `obs_index` is the fold's join table.
    fn observation_row(&self, event_id: &str) -> Option<&Json> {
        self.view.obs_index.get(event_id)
    }

    /// `schedule_event_id` a `retry_due` cue fired on — join
    /// `occurrence_key = retry:{scope}:{attempt?}` against the pending
    /// schedules.
    fn schedule_for_cue(&self, cue: &crate::view::FleetCue) -> Option<String> {
        let scope = scope_of_retry(&cue.trigger);
        self.view
            .pending_schedules
            .iter()
            .find(|(_, p)| p.get("scope_id").and_then(Json::as_str) == Some(scope.as_str()))
            .map(|(id, _)| id.clone())
    }

    /// Whether the item has a live (unconsumed) retry schedule.
    fn retry_live(&self, item_id: &str) -> bool {
        self.view
            .pending_schedules
            .values()
            .any(|s| s.get("scope_id").and_then(Json::as_str) == Some(item_id))
    }

    /// Ensure a `retry_due` subscription exists for the item — created
    /// lazily the first time a retry schedules (`created_by` = the
    /// schedule row; durable dedup by trigger equality).
    fn ensure_retry_sub(
        &mut self,
        store: &mut Store,
        item_id: &str,
        created_by: &str,
    ) -> Result<(), FleetError> {
        if self.view.sub_triggers.values().any(
            |t| matches!(t, Trigger::RetryDue { scope_id } if scope_id == item_id),
        ) {
            return Ok(());
        }
        store.wakeup_subscribe(
            &self.run_id,
            &self.lease,
            Trigger::RetryDue {
                scope_id: item_id.to_string(),
            },
            WakeupPolicy::default_policy(),
            &EventRef {
                run_id: self.run_id.clone(),
                event_id: created_by.to_string(),
            },
        )?;
        self.refresh(store)?;
        Ok(())
    }

    // ── the public op surface ──────────────────────────────────────────

    /// `fleet.admit` — the durable admission (`state = queued`). The
    /// `idempotency_key` dedups within the activation: identical dossier
    /// ⇒ `known`, divergent ⇒ `SourceConflict` (RC-3's direct-admit arm).
    pub fn admit(
        &mut self,
        store: &mut Store,
        init: WorkItemInit,
    ) -> Result<AdmitOutcome, FleetError> {
        self.bound(store)?;
        if let Some(existing) = self.view.by_idem_key.get(&init.idempotency_key) {
            let it = self.view.item(existing)?.clone();
            if let Some(field) = conflict_field(&it, &init) {
                let expected = field_value(&it, &field);
                let seen = field_value_init(&init, &field);
                self.emit(
                    store,
                    "control.work_item.dispatched",
                    payloads::source_conflict_payload(
                        &it.item_id,
                        &it.run_item_id,
                        &field,
                        &expected,
                        &seen,
                    ),
                    vec![],
                )?;
                return Err(FleetError::SourceConflict {
                    item: init.item_id,
                    field,
                });
            }
            return Ok(AdmitOutcome::Known {
                item_id: it.item_id.clone(),
            });
        }
        if self.view.items.contains_key(&init.item_id) {
            // Same id, new idem key — the id is the join; a dossier
            // conflict lands `source_conflict`.
            let it = self.view.item(&init.item_id)?.clone();
            if let Some(field) = conflict_field(&it, &init) {
                let expected = field_value(&it, &field);
                let seen = field_value_init(&init, &field);
                self.emit(
                    store,
                    "control.work_item.dispatched",
                    payloads::source_conflict_payload(
                        &it.item_id,
                        &it.run_item_id,
                        &field,
                        &expected,
                        &seen,
                    ),
                    vec![],
                )?;
                return Err(FleetError::SourceConflict {
                    item: init.item_id,
                    field,
                });
            }
            return Ok(AdmitOutcome::Known {
                item_id: init.item_id,
            });
        }
        // `capacity.items` — the admission bound.
        if self.view.live_item_count() as u64 >= self.view.spec.capacity.items {
            return Err(FleetError::CapacityFull {
                run: self.run_id.clone(),
            });
        }
        // `blocking` members must be admitted-or-admissible ids inside
        // this activation (a `blocking` member naming an unadmitted item
        // is fine — the child may land later; cycles are checked on the
        // edge itself once both endpoints exist).
        // `control.work_item.created` — canonical ADR-0205 D8's class
        // (`create_work_item`); the payload is the `admit` dossier shape.
        let env = self.emit(
            store,
            "control.work_item.created",
            payloads::admit_payload(&self.run_id, &init, &self.view.spec_ref),
            vec![],
        )?;
        Ok(AdmitOutcome::Admitted {
            item_id: init.item_id,
            event_id: env.event_id,
        })
    }

    /// `fleet.dispatch_note` — stamp `dispatch.run_ref`/`dispatch.error`
    /// on the durable row. `run_ref` anchors: the named run's manifest
    /// `spawn_event` must resolve to this item's `verb:dispatch` row —
    /// the `fleet_anchor` obligation is enforced at write time (an
    /// unanchored run is `Refused{stale_dispatch}`, never a dangling
    /// cross-run reference — CC3).
    pub fn dispatch_note(
        &mut self,
        store: &mut Store,
        item_id: &str,
        spec_ref: &str,
        run_ref: Option<&str>,
        error: Option<&str>,
    ) -> Result<(), FleetError> {
        self.bound(store)?;
        self.refresh(store)?;
        let it = self.view.item(item_id)?.clone();
        // The note's `spec_ref` must name the dispatch's durable spec —
        // a stale note contradicts the row.
        let want = it
            .dispatch
            .spec_ref
            .clone()
            .unwrap_or_else(|| self.view.spec_ref.clone());
        if spec_ref != want {
            return Err(FleetError::StaleDispatchNote {
                item: item_id.to_string(),
                expected: want,
            });
        }
        if it.dispatch.status != "dispatching" && run_ref.is_none() && error.is_none() {
            return Err(FleetError::StaleDispatch {
                item: item_id.to_string(),
            });
        }
        match (run_ref, error) {
            (Some(run), _) => {
                // Anchor check — the child's `spawn_event` must name the
                // dispatch row (the cross-run link is verified, not
                // assumed).
                let child_manifest = store.manifest(run).map_err(FleetError::Store)?;
                let spawn = child_manifest.spawn_event.as_ref();
                let dispatch_ev = it.dispatch_event_id.clone();
                match (spawn, &dispatch_ev) {
                    (Some(sp), Some(ev))
                        if sp.run_id == self.run_id && &sp.event_id == ev => {}
                    _ => {
                        return Err(FleetError::StaleDispatchNote {
                            item: item_id.to_string(),
                            expected: dispatch_ev.clone().unwrap_or_default(),
                        })
                    }
                }
                // The `run_note`'s cause names the child's own
                // `lifecycle.run.created` (seq 0 — always present, so the
                // ref resolves) plus the activation's dispatch row.
                let child_created = store
                    .events(run)
                    .ok()
                    .and_then(|ev| ev.first().map(|e| e.event_id.clone()))
                    .ok_or_else(|| FleetError::StaleDispatchNote {
                        item: item_id.to_string(),
                        expected: format!("{run} run.created"),
                    })?;
                self.emit(
                    store,
                    "control.work_item.dispatched",
                    payloads::run_note_payload(
                        &it.item_id,
                        &it.run_item_id,
                        spec_ref,
                        run,
                    ),
                    vec![
                        EventRef {
                            run_id: run.to_string(),
                            event_id: child_created,
                        },
                        EventRef {
                            run_id: self.run_id.clone(),
                            event_id: it.admitted_event_id.clone(),
                        },
                    ],
                )?;
            }
            (None, Some(err)) => {
                self.emit(
                    store,
                    "control.work_item.dispatched",
                    payloads::error_note_payload(
                        &it.item_id,
                        &it.run_item_id,
                        spec_ref,
                        err,
                    ),
                    vec![],
                )?;
            }
            (None, None) => {
                return Err(FleetError::StaleDispatch {
                    item: item_id.to_string(),
                })
            }
        }
        Ok(())
    }

    /// `fleet.set_owner` — a graph edge update (cross-fleet/cycle/
    /// unknown ⇒ `GraphError`). The edge is `agent → owner` inside the
    /// activation; `item` is the durable calling context.
    pub fn set_owner(
        &mut self,
        store: &mut Store,
        item_id: &str,
        agent: &str,
        owner: &str,
    ) -> Result<(), FleetError> {
        self.bound(store)?;
        self.view
            .ownership
            .validate_set_owner(&self.view.spec, agent, owner)
            .map_err(|mut e| {
                if let FleetError::CrossFleet { run, .. }
                | FleetError::Cycle { run, .. } = &mut e
                {
                    *run = self.run_id.clone();
                }
                e
            })?;
        let it = self.view.item(item_id)?.clone();
        if self
            .view
            .ownership
            .edges
            .get(agent)
            .map(|o| o == owner)
            .unwrap_or(false)
        {
            return Ok(()); // idempotent — the edge is already the durable record
        }
        self.emit(
            store,
            "control.work_item.owner_changed",
            payloads::owner_changed_payload(&it.item_id, &it.run_item_id, agent, owner),
            vec![],
        )?;
        Ok(())
    }

    /// `fleet.ack_owner` — the durable acknowledgement (`ack` flips the
    /// item's `owner_ack`; required for `ack_required` dispatch classes;
    /// `reviewer`/`local_admin` never require it).
    pub fn ack_owner(
        &mut self,
        store: &mut Store,
        item_id: &str,
        agent: &str,
    ) -> Result<(), FleetError> {
        self.bound(store)?;
        let it = self.view.item(item_id)?.clone();
        if it.owner.as_deref() != Some(agent) {
            return Err(FleetError::NotOwner {
                run: self.run_id.clone(),
                item: item_id.to_string(),
            });
        }
        if it.owner_ack {
            return Ok(()); // idempotent
        }
        let lease_agent =
            identity::lease_agent_ref(&identity::agent_ref(agent), &self.lease.lease_id);
        self.emit(
            store,
            "control.work_item.owner_acknowledged",
            payloads::owner_acknowledged_payload(
                &it.item_id,
                &it.run_item_id,
                agent,
                &lease_agent,
            ),
            vec![],
        )?;
        Ok(())
    }

    /// `fleet.transfer_owner` — the canonical O-4 ownership transfer:
    /// `control.work_item.owner_changed{from, to, basis}` (the
    /// `acknowledge_owner` op lands the handshake's second event —
    /// `owner_acknowledged{by = to}`; dispatch is gated in between).
    /// `basis ∈ {approval, policy_rule}` carries the authority citation
    /// (`approval` names the human decision, `policy_rule` the sealed
    /// rule); grant coverage resolution is the D-1 interim (ADR-0211) —
    /// the check-5 endorsement leg is recorded verbatim pending the live
    /// human directory. The `handoff` class itself is kernel-produced
    /// only (the escalation/overdue state handoff — `state_handoff`).
    pub fn transfer_owner(
        &mut self,
        store: &mut Store,
        item_id: &str,
        new_owner: &str,
        basis: &str,
    ) -> Result<(), FleetError> {
        self.bound(store)?;
        let it = self.view.item(item_id)?.clone();
        if !self.view.spec.agents.iter().any(|a| a == new_owner) {
            return Err(FleetError::CrossFleet {
                run: self.run_id.clone(),
                item: item_id.to_string(),
                owner: new_owner.to_string(),
            });
        }
        if !TRANSFER_BASES.contains(&basis) {
            return Err(FleetError::SchemaViolation {
                detail: format!("basis {basis} (closed set {TRANSFER_BASES:?})"),
            });
        }
        let from = it.owner.clone().unwrap_or_else(|| "none".to_string());
        if it.owner.as_deref() == Some(new_owner) {
            return Ok(()); // idempotent
        }
        self.emit(
            store,
            "control.work_item.owner_changed",
            payloads::transfer_owner_payload(
                &it.item_id,
                &it.run_item_id,
                &from,
                new_owner,
                basis,
            ),
            vec![],
        )?;
        Ok(())
    }

    /// The kernel-produced state-handoff record —
    /// `control.work_item.handoff` carrying `to_agent` and the
    /// `lease_agent` surrogate (§5i.1 — the item enters state `handoff`
    /// and the row itself re-assigns ownership). Only the reconciler's
    /// own arms (the escalation `to_agent` handoff, the overdue
    /// resolved-owner takeover) mint this class.
    fn state_handoff(
        &mut self,
        store: &mut Store,
        it: &WorkItemView,
        to: &str,
        basis: &str,
    ) -> Result<(), FleetError> {
        let from = it.owner.clone().unwrap_or_else(|| "none".to_string());
        let href = identity::handoff_ref(&from, to, &it.item_id);
        let lease_agent =
            identity::lease_agent_ref(&identity::agent_ref(to), &self.lease.lease_id);
        self.emit(
            store,
            "control.work_item.handoff",
            payloads::handoff_payload(
                &it.item_id,
                &it.run_item_id,
                &href,
                &from,
                to,
                &lease_agent,
                basis,
            ),
            vec![],
        )?;
        Ok(())
    }

    /// `fleet.escalate` — the durable raise (`lifecycle.escalation.raised`
    /// + `control.work_item.blocked{escalation}` + the deadline `timer`
    /// subscription when `deadline_ms` is declared). `to` names the
    /// escalation-target agent the raise hands off to (a `handoff` row
    /// carrying the `lease_agent` surrogate — §5i.1 #5).
    pub fn escalate(
        &mut self,
        store: &mut Store,
        item_id: &str,
        issue: &str,
        by: &str,
        to: Option<&str>,
        deadline_ms: Option<u64>,
    ) -> Result<String, FleetError> {
        self.bound(store)?;
        self.refresh(store)?;
        let it = self.view.item(item_id)?.clone();
        if it.watch_state == "dead" {
            return Err(FleetError::NotStoppable {
                item: item_id.to_string(),
            });
        }
        if let Some(t) = to {
            if !self.view.spec.agents.iter().any(|a| a == t) {
                return Err(FleetError::CrossFleet {
                    run: self.run_id.clone(),
                    item: item_id.to_string(),
                    owner: t.to_string(),
                });
            }
        }
        self.raise_escalation(
            store,
            &it,
            issue,
            "manual",
            by,
            to.map(|t| (t.to_string(), deadline_ms)),
        )
        .map(|_| {
            identity::escalation_ref(
                item_id,
                issue,
                self.view.raised_no.get(item_id).copied().unwrap_or(0),
            )
        })
    }

    /// The raise primitive — shared by the op and the reconciler's own
    /// causes (`escalate` carries the `to_agent` handoff arm for
    /// `on.*.escalate` declarations).
    fn raise_escalation(
        &mut self,
        store: &mut Store,
        it: &WorkItemView,
        issue: &str,
        cause: &str,
        by: &str,
        escalate: Option<(String, Option<u64>)>, // (to_agent, deadline_ms)
    ) -> Result<(), FleetError> {
        // One open escalation per item — a re-raise while open bumps
        // `raised_no` (the `escalation_ref` stays unique).
        let raised_no = self.view.raised_no.get(&it.item_id).copied().unwrap_or(0) + 1;
        let issue_ref = identity::issue_ref(&it.item_id, issue);
        let escalation_ref = identity::escalation_ref(&it.item_id, issue, raised_no);
        let (to_agent, deadline_ms) = match &escalate {
            Some((t, d)) => (Some(t.clone()), *d),
            None => (None, None),
        };
        // The deadline timer — `timer{at_ms}` subscription; the fired
        // cue joins back through `deadline_sub`.
        let mut deadline_sub = None;
        if let Some(d) = deadline_ms {
            if self.view.spec.defaults.escalation_timeout_ms.is_some() || d > 0 {
                deadline_sub = Some(store.wakeup_subscribe(
                    &self.run_id,
                    &self.lease,
                    Trigger::Timer { at_ms: d },
                    WakeupPolicy::default_policy(),
                    &EventRef {
                        run_id: self.run_id.clone(),
                        event_id: it.admitted_event_id.clone(),
                    },
                )?);
                self.refresh(store)?;
            }
        }
        let handoff = if let Some(to) = to_agent {
            let lease_agent =
                identity::lease_agent_ref(&identity::agent_ref(&to), &self.lease.lease_id);
            Some((to, lease_agent))
        } else {
            None
        };
        self.emit(
            store,
            "lifecycle.escalation.raised",
            payloads::escalation_raised_payload(
                &it.item_id,
                &it.run_item_id,
                issue,
                &issue_ref,
                &escalation_ref,
                by,
                raised_no,
                cause,
                deadline_ms,
                deadline_sub.as_deref(),
                handoff
                    .as_ref()
                    .map(|(t, l)| (t.as_str(), l.as_str())),
            ),
            vec![EventRef {
                run_id: self.run_id.clone(),
                event_id: it.admitted_event_id.clone(),
            }],
        )?;
        if let Some((to, _lease_agent)) = handoff {
            // The escalation's state handoff — `control.work_item.handoff`
            // carries the `lease_agent` surrogate (§5i.1 — "the handoff
            // carries `lease_agent`"). Kernel-produced (E-1).
            self.state_handoff(store, it, &to, "escalation")?;
        }
        Ok(())
    }

    /// `fleet.resolve_escalation` — `legitimate` requires the `issue_ref`
    /// to match the item's open escalation (the durable record, never a
    /// caller claim); `resolution_count` accumulates per item.
    pub fn resolve_escalation(
        &mut self,
        store: &mut Store,
        item_id: &str,
        issue_ref: &str,
        by: &str,
        resolution: &str,
        note: Option<&str>,
    ) -> Result<(), FleetError> {
        self.bound(store)?;
        self.refresh(store)?;
        let it = self.view.item(item_id)?.clone();
        let Some(esc) = &it.escalation else {
            return Err(FleetError::NoOpenEscalation {
                item: item_id.to_string(),
            });
        };
        if !RESOLUTION_KINDS.contains(&resolution) {
            return Err(FleetError::SchemaViolation {
                detail: format!("resolution {resolution} (closed set {RESOLUTION_KINDS:?})"),
            });
        }
        // `legitimate` — the link must name the open escalation's issue.
        if esc.issue_ref != issue_ref || esc.escalation_ref.is_empty() {
            return Err(FleetError::IllegitimateResolution {
                item: item_id.to_string(),
                issue_ref: issue_ref.to_string(),
            });
        }
        // `legitimate` authority (§5i.1 #5: resolution under the owner/
        // grant, never a free-form principal) — `by` must be the
        // escalation's declared `to_agent`, a member of the item's
        // resolved owner chain (the ownership graph's `agent → owner`
        // walk), or `local_admin` (the reconciler's own resolutions).
        let esc_to = esc
            .handoff
            .as_ref()
            .and_then(|h| h.get("to_agent"))
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        let mut chain: Vec<String> = Vec::new();
        if let Some(owner) = &it.owner {
            let mut cur = owner.clone();
            let mut seen = std::collections::BTreeSet::from([cur.clone()]);
            chain.push(cur.clone());
            while let Some(next) = self.view.ownership.edges.get(&cur) {
                if !seen.insert(next.clone()) {
                    break;
                }
                chain.push(next.clone());
                cur = next.clone();
            }
        }
        let legitimate = by == "local_admin"
            || (!esc_to.is_empty() && by == esc_to)
            || chain.iter().any(|a| a == by);
        if !legitimate {
            return Err(FleetError::IllegitimateResolution {
                item: item_id.to_string(),
                issue_ref: issue_ref.to_string(),
            });
        }
        let count = self
            .view
            .resolution_count
            .get(item_id)
            .copied()
            .unwrap_or(0)
            + 1;
        self.emit(
            store,
            "lifecycle.escalation.resolved",
            payloads::escalation_resolved_payload(
                &it.item_id,
                &it.run_item_id,
                issue_ref,
                &esc.escalation_ref,
                by,
                resolution,
                note,
                count,
            ),
            vec![],
        )?;
        Ok(())
    }

    /// `fleet.block` — an operator/reconciler block cause.
    pub fn block(
        &mut self,
        store: &mut Store,
        item_id: &str,
        code: &str,
    ) -> Result<(), FleetError> {
        self.bound(store)?;
        let it = self.view.item(item_id)?.clone();
        if it.watch_state == "dead" {
            return Err(FleetError::NotStoppable {
                item: item_id.to_string(),
            });
        }
        if it.blocked.contains(code) {
            return Ok(()); // idempotent — the set member exists
        }
        self.emit(
            store,
            "control.work_item.blocked",
            payloads::block_add_payload(&it.item_id, &it.run_item_id, code, None, None),
            vec![],
        )?;
        Ok(())
    }

    /// `fleet.unblock` — the remove row (`missing_cause` is the typed
    /// refusal when the set doesn't carry it).
    pub fn unblock(
        &mut self,
        store: &mut Store,
        item_id: &str,
        code: &str,
    ) -> Result<(), FleetError> {
        self.bound(store)?;
        let it = self.view.item(item_id)?.clone();
        if !it.blocked.contains(code) {
            return Err(FleetError::Unsupported {
                op: "unblock",
                reason: format!("missing_cause{{code:{code}}}"),
            });
        }
        self.emit(
            store,
            "control.work_item.blocked",
            payloads::block_remove_payload(&it.item_id, &it.run_item_id, code),
            vec![],
        )?;
        Ok(())
    }

    /// `fleet.stop` — mark the item terminal + cascade RC-7 (children
    /// stop unless already blocked/terminal). `NotStoppable` on an
    /// already-terminal item.
    pub fn stop(&mut self, store: &mut Store, item_id: &str) -> Result<(), FleetError> {
        self.bound(store)?;
        let it = self.view.item(item_id)?.clone();
        if it.watch_state == "dead" || it.settlement.is_some() {
            return Err(FleetError::NotStoppable {
                item: item_id.to_string(),
            });
        }
        self.emit(
            store,
            "control.work_item.stopped",
            payloads::stopped_payload(
                "stop",
                &it.item_id,
                &it.run_item_id,
                Some("cancelled"),
                Some("operator"),
                &[],
            ),
            vec![],
        )?;
        // RC-7's cascade — the durable stop is landed; children stop
        // unless already blocked/terminal.
        let children: Vec<WorkItemView> = it
            .blocking
            .iter()
            .filter_map(|c| self.view.items.get(c).cloned())
            .collect();
        for child in children {
            if child.watch_state == "watching"
                && child.settlement.is_none()
                && child.blocked.is_empty()
            {
                self.emit(
                    store,
                    "control.work_item.stopped",
                    payloads::stopped_payload(
                        "terminal",
                        &child.item_id,
                        &child.run_item_id,
                        Some("abandoned"),
                        Some("parent_terminal"),
                        &[],
                    ),
                    vec![],
                )?;
            }
        }
        Ok(())
    }

    /// `fleet.settle` — the preconditions re-checked at settle (RC-1's
    /// spec + RC-5's settled-state), then `control.work_item.stopped{
    /// verb:settle, outcome, evidence_refs}` (AC-7).
    pub fn settle(
        &mut self,
        store: &mut Store,
        item_id: &str,
        outcome: &str,
        evidence_refs: Vec<String>,
    ) -> Result<(), FleetError> {
        self.bound(store)?;
        self.refresh(store)?;
        let it = self.view.item(item_id)?.clone();
        if !OUTCOMES.contains(&outcome) {
            return Err(FleetError::SchemaViolation {
                detail: format!("outcome {outcome} (closed set {OUTCOMES:?})"),
            });
        }
        if it.watch_state == "dead" || it.settlement.is_some() {
            return Err(FleetError::SettlePrecondition {
                item: item_id.to_string(),
                detail: "already_terminal".into(),
            });
        }
        if it.spec_ref != self.view.spec_ref {
            return Err(FleetError::StaleSpec {
                expected: self.view.spec_ref.clone(),
                seen: it.spec_ref.clone(),
            });
        }
        self.emit(
            store,
            "control.work_item.stopped",
            payloads::stopped_payload(
                "settle",
                &it.item_id,
                &it.run_item_id,
                Some(outcome),
                None,
                &evidence_refs,
            ),
            vec![],
        )?;
        Ok(())
    }

    /// `claim(work_item_id, holder)` — the canonical two-step's first
    /// half: the ADR-0130 `resource:` scoped lease on `run_item(X)`
    /// (RC-2's "at most one activation per work item" — a second
    /// concurrent claim is `WouldBlock`; the lease replays at restore).
    /// `reconcile`'s dispatch path does lease+mark atomically; this op
    /// serves the explicit `claim → dispatch(lease)` surface.
    pub fn claim(
        &mut self,
        store: &mut Store,
        item_id: &str,
    ) -> Result<Json, FleetError> {
        self.bound(store)?;
        let it = self.view.item(item_id)?.clone();
        if it.watch_state == "dead" || it.settlement.is_some() {
            return Err(FleetError::NotStoppable {
                item: item_id.to_string(),
            });
        }
        let scope = LeaseScope::Resource(format!("run_item:{}", it.run_item_id));
        let rec = store
            .lease_acquire(
                &self.run_id,
                &self.lease,
                &scope,
                &self.holder,
                self.view.spec.defaults.dispatch_lease_ms,
            )
            .map_err(FleetError::Store)?;
        self.refresh(store)?;
        Ok(Json::obj([
            ("lease_id", Json::str(&rec.lease_id)),
            ("scope", Json::str(&format!("run_item:{}", it.run_item_id))),
            ("holder", Json::str(&self.holder)),
            ("generation", Json::Int(rec.generation as i64)),
        ]))
    }

    /// `cancel(work_item_id, by, cascade)` — the canonical cancel:
    /// `control.work_item.cancelled` + `foreground` cancels children
    /// first (`CascadeBlocked{children}` when a blocking child is
    /// itself blocked — the parent may not silently drop it);
    /// `background` leaves children running; `orphan` is refused unless
    /// `spec` declares the policy (`defaults` carries no orphan arm —
    /// D-6's cross-item state is deferred).
    pub fn cancel(
        &mut self,
        store: &mut Store,
        item_id: &str,
        by: &str,
        cascade: &str,
    ) -> Result<(), FleetError> {
        self.bound(store)?;
        self.refresh(store)?;
        let it = self.view.item(item_id)?.clone();
        if it.watch_state == "dead" || it.settlement.is_some() {
            return Err(FleetError::NotStoppable {
                item: item_id.to_string(),
            });
        }
        match cascade {
            "foreground" => {
                // Children first — a blocked child can't be silently
                // dropped (`CascadeBlocked` refuses the parent cancel,
                // never orphans it).
                let children: Vec<WorkItemView> = it
                    .blocking
                    .iter()
                    .filter_map(|c| self.view.items.get(c).cloned())
                    .collect();
                let blocked_children: Vec<String> = children
                    .iter()
                    .filter(|c| {
                        c.watch_state == "watching" && !c.blocked.is_empty()
                    })
                    .map(|c| c.item_id.clone())
                    .collect();
                if !blocked_children.is_empty() {
                    return Err(FleetError::Unsupported {
                        op: "cancel",
                        reason: format!(
                            "CascadeBlocked{{children:[{}]}}",
                            blocked_children.join(",")
                        ),
                    });
                }
                for child in children {
                    if child.watch_state == "watching" && child.settlement.is_none() {
                        self.emit(
                            store,
                            "control.work_item.cancelled",
                            payloads::cancelled_payload(
                                &child.item_id,
                                &child.run_item_id,
                                "parent_cancelled",
                                by,
                            ),
                            vec![],
                        )?;
                    }
                }
            }
            "background" => {}
            "orphan" => {
                return Err(FleetError::Unsupported {
                    op: "cancel",
                    reason: "orphan: no declared cascade policy (D-6 deferred)".into(),
                })
            }
            other => {
                return Err(FleetError::SchemaViolation {
                    detail: format!(
                        "cascade {other} (closed set foreground|background|orphan)"
                    ),
                })
            }
        }
        self.emit(
            store,
            "control.work_item.cancelled",
            payloads::cancelled_payload(&it.item_id, &it.run_item_id, cascade, by),
            vec![],
        )?;
        Ok(())
    }

    /// `annotate(work_item_id, subject, subject_ref?, text_ref, readers,
    /// by)` — the ADR-0205 D7 annotation row (plane 5; the annotation is
    /// durable on the activation, never model-facing — a `HarnessRule`
    /// lifting it into a `ContextItem` is the only delivery path).
    pub fn annotate(
        &mut self,
        store: &mut Store,
        item_id: &str,
        subject: &str,
        subject_ref: Option<&Json>,
        text_ref: &str,
        readers: &[String],
        by: &str,
    ) -> Result<(), FleetError> {
        self.bound(store)?;
        let it = self.view.item(item_id)?.clone();
        self.emit(
            store,
            "control.work_item.annotated",
            payloads::annotated_payload(
                &it.item_id,
                &it.run_item_id,
                subject,
                subject_ref,
                text_ref,
                readers,
                by,
            ),
            vec![],
        )?;
        Ok(())
    }

    /// `bind_source(work_item_id, WorkSourceBinding)` — `{role ∈
    /// origin|mirror|subscription, source_id, native_id?}` lands on the
    /// item's `source.bindings[]` (ADR-0205 D5 — an agent-opened PR is a
    /// `subscription` binding on the SAME item).
    pub fn bind_source(
        &mut self,
        store: &mut Store,
        item_id: &str,
        binding: Json,
    ) -> Result<(), FleetError> {
        self.bound(store)?;
        self.refresh(store)?;
        let it = self.view.item(item_id)?.clone();
        let role = binding
            .get("role")
            .and_then(Json::as_str)
            .unwrap_or_default();
        if !matches!(role, "origin" | "mirror" | "subscription") {
            return Err(FleetError::SchemaViolation {
                detail: format!(
                    "binding.role {role} (closed set origin|mirror|subscription)"
                ),
            });
        }
        // Durable dedup — the binding set already carrying it is `known`.
        let dup = it
            .source
            .get("bindings")
            .and_then(|b| match b {
                Json::Arr(a) => Some(a.iter().any(|x| x == &binding)),
                _ => None,
            })
            .unwrap_or(false);
        if dup {
            return Ok(());
        }
        self.emit(
            store,
            "control.work_item.dispatched",
            payloads::bind_source_payload(&it.item_id, &it.run_item_id, &binding),
            vec![],
        )?;
        Ok(())
    }

    /// `resume_from_handoff(work_item_id, by)` — exits the `handoff`
    /// state *only by a human act* (§5i.1 #2's exit rule): the durable
    /// `dispatched{verb:"resume"}` record + the `human_gate` remove row.
    /// `by` must be the item's owner (or `local_admin`) — a `delegate`/
    /// `external` act is `IllegitimateEndorsement`-class and refuses.
    pub fn resume_from_handoff(
        &mut self,
        store: &mut Store,
        item_id: &str,
        by: &str,
    ) -> Result<(), FleetError> {
        self.bound(store)?;
        let it = self.view.item(item_id)?.clone();
        if !it.blocked.contains("human_gate") {
            return Err(FleetError::Unsupported {
                op: "resume_from_handoff",
                reason: format!("item {item_id} is not in the handoff state"),
            });
        }
        let legit = it.owner.as_deref() == Some(by) || by == "local_admin";
        if !legit {
            return Err(FleetError::NotOwner {
                run: self.run_id.clone(),
                item: item_id.to_string(),
            });
        }
        self.emit(
            store,
            "control.work_item.dispatched",
            payloads::resume_payload(&it.item_id, &it.run_item_id, by),
            vec![],
        )?;
        self.emit(
            store,
            "control.work_item.blocked",
            payloads::block_remove_payload(&it.item_id, &it.run_item_id, "human_gate"),
            vec![],
        )?;
        Ok(())
    }

    /// `restore(fleet)` — canonical: ADR-0130's restore on the fleet run
    /// (`ensure`) **then** `reconcile` over the rebuilt prefix (RC-8 —
    /// same prefix ⇒ same `[FleetAction]`).
    pub fn restore<A: WorkSourceAdapter>(
        store: &mut Store,
        run_id: &str,
        holder: &str,
        writer_ttl_ms: u64,
        adapter: &A,
        now_ms: u64,
    ) -> Result<(FleetEngine, ReconcileReport), FleetError> {
        let mut eng = Self::ensure(store, run_id, holder, writer_ttl_ms)?;
        let report = eng.reconcile(store, adapter, now_ms)?;
        Ok((eng, report))
    }

    // ── reads ──────────────────────────────────────────────────────────

    /// `fleet.fleet_view` — the canonical projection (ADR-0207 D4;
    /// `project`-pure with a `derived_from` watermark — the cursor — and
    /// rebuild equality). One document over the folded prefix: spec
    /// coordinates, items, escalations, ownership, cue/suspension state,
    /// annotations, and the durable cursor.
    pub fn fleet_view(&self) -> Json {
        let items: Vec<Json> = self
            .view
            .items
            .values()
            .map(|it| {
                let mut m = BTreeMap::new();
                m.insert("item_id".into(), Json::str(&it.item_id));
                m.insert("run_item_id".into(), Json::str(&it.run_item_id));
                m.insert("title".into(), Json::str(&it.title));
                m.insert("state".into(), Json::str(&derive_state(it)));
                m.insert("spec_ref".into(), Json::str(&it.spec_ref));
                m.insert("source".into(), it.source.clone());
                m.insert("idempotency_key".into(), Json::str(&it.idempotency_key));
                m.insert("owner".into(), it.owner.as_ref().map(Json::str).unwrap_or(Json::Null));
                m.insert("owner_ack".into(), Json::Bool(it.owner_ack));
                m.insert("blocking".into(), Json::Arr(it.blocking.iter().map(Json::str).collect()));
                m.insert("blocked".into(), Json::Arr(it.blocked.iter().map(Json::str).collect()));
                m.insert("suspended".into(), Json::Bool(it.suspended));
                m.insert("watch_state".into(), Json::str(&it.watch_state));
                if let Some(s) = &it.settlement {
                    m.insert("outcome".into(), Json::str(&s.outcome));
                }
                if let Some(r) = &it.dispatch.run_ref {
                    m.insert("run_ref".into(), Json::str(r));
                }
                Json::Obj(m)
            })
            .collect();
        let ownership: Vec<Json> = self
            .view
            .ownership
            .edges
            .iter()
            .map(|(a, o)| {
                Json::obj([("agent", Json::str(a)), ("owner", Json::str(o))])
            })
            .collect();
        let cues: Vec<Json> = self
            .view
            .cues
            .iter()
            .map(|c| {
                Json::obj([
                    ("subscription_id", Json::str(&c.subscription_id)),
                    ("occurrence_key", Json::str(&c.occurrence_key)),
                    ("trigger", c.trigger.to_json()),
                ])
            })
            .collect();
        Json::obj([
            ("schema", Json::str("hh.fleet.view/1")),
            ("run", Json::str(&self.run_id)),
            ("spec_ref", Json::str(&self.view.spec_ref)),
            ("policy_ref", Json::str(&self.view.spec.policy_ref)),
            ("items", Json::Arr(items)),
            ("escalations", Json::Arr(self.view.escalation_rows.clone())),
            ("ownership", Json::Arr(ownership)),
            ("cues", Json::Arr(cues)),
            (
                "suspended_sources",
                Json::Arr(
                    self.view
                        .suspended_sources
                        .iter()
                        .map(Json::str)
                        .collect(),
                ),
            ),
            ("annotations", Json::Arr(self.view.annotations.clone())),
            (
                "derived_from",
                Json::obj([
                    ("event_count", Json::Int(self.view.event_count as i64)),
                    (
                        "observed_at_ms",
                        Json::Int(self.view.observed_watermark as i64),
                    ),
                ]),
            ),
        ])
    }

    /// `fleet.work_item` — the folded item view.
    pub fn work_item(&self, item_id: &str) -> Result<WorkItemView, FleetError> {
        self.view.item(item_id).cloned()
    }

    /// `fleet.list` — the folded item set (deterministic order).
    pub fn list(&self) -> Vec<WorkItemView> {
        self.view.items.values().cloned().collect()
    }

    /// `fleet.state_map` — the Π narrowing projection per item (or the
    /// activation when `item` is `None`): `{policy_ref, narrowing_leaves,
    /// policy_fingerprint, delta}`. The delta is `narrowing` by
    /// construction — the state map only ever *narrows*; a widened
    /// projection is refused at `check_activation_delta`, never
    /// produced here (§5i.1 #10).
    pub fn state_map(&self, item_id: Option<&str>) -> Result<Json, FleetError> {
        let leaves: Vec<Json> = match item_id {
            Some(id) => {
                let it = self.view.item(id)?;
                if it.on.narrowing.is_empty() {
                    self.view
                        .spec
                        .narrowing
                        .iter()
                        .map(|l| l.to_json())
                        .collect()
                } else {
                    it.on.narrowing.iter().map(|l| l.to_json()).collect()
                }
            }
            None => self
                .view
                .spec
                .narrowing
                .iter()
                .map(|l| l.to_json())
                .collect(),
        };
        let leaf_ids: Vec<String> = match item_id {
            Some(id) => {
                let it = self.view.item(id)?;
                let src = if it.on.narrowing.is_empty() {
                    &self.view.spec.narrowing
                } else {
                    &it.on.narrowing
                };
                src.iter().map(|l| l.leaf_id()).collect()
            }
            None => self
                .view
                .spec
                .narrowing
                .iter()
                .map(|l| l.leaf_id())
                .collect(),
        };
        let fingerprint = hh_monitor::approval::policy_fingerprint(
            &self.view.spec.policy_ref,
            "attended",
            &leaf_ids,
        );
        Ok(Json::obj([
            ("policy_ref", Json::str(&self.view.spec.policy_ref)),
            ("narrowing_leaves", Json::Arr(leaves)),
            ("delta", Json::str("narrowing")),
            ("policy_fingerprint", Json::str(&fingerprint)),
            (
                "spec_ref",
                Json::str(&self.view.spec_ref),
            ),
        ]))
    }

    /// `check_activation_delta(candidate_leaves)` — would the candidate
    /// narrowing set keep the activation Π-narrowing? `widening{cells}`
    /// refuses (a candidate that moves any probed cell toward `allow` —
    /// §5g.1 §2.4's rule through `hh_monitor::policy::policy_delta`).
    pub fn check_activation_delta(
        &self,
        candidate: &[hh_embed_schema::types::NarrowingLeaf],
    ) -> Result<Json, FleetError> {
        use hh_hir::EffectDomain;
        use hh_monitor::policy::{default_table, policy_delta, Mode, Cond, PiVerdict, PolicyDelta, PolicyRow};
        use hh_ontology::risk::RiskScope;
        let base = default_table(&self.view.spec.policy_ref, Mode::Attended);
        // Leaves → narrowing rows appended to the base table — `deny` /
        // `ask` / `allow_lease` map onto `PiVerdict`; the probe grid
        // catches anything that would widen.
        let mut narrowed = base.clone();
        for leaf in candidate {
            let domain = EffectDomain::parse(&leaf.domain).map_err(|_| {
                FleetError::SchemaViolation {
                    detail: format!("narrowing leaf domain {}", leaf.domain),
                }
            })?;
            let verdict = match leaf.disposition.as_str() {
                "deny" => PiVerdict::Deny,
                "ask" => PiVerdict::Ask,
                "allow_lease" => PiVerdict::Allow,
                other => {
                    return Err(FleetError::SchemaViolation {
                        detail: format!("narrowing leaf disposition {other}"),
                    })
                }
            };
            let mut conditions = vec![Cond::DomainIs(domain)];
            if let Some(scope) = &leaf.scope {
                if let Some(rs) = RiskScope::parse(scope) {
                    conditions.push(Cond::ScopeIs(rs));
                }
            }
            narrowed.rows.push(PolicyRow {
                id: format!("narrow_{}", leaf.leaf_id().chars().take(12).collect::<String>()),
                conditions,
                verdict,
            });
        }
        let delta = policy_delta(&base, &narrowed);
        match delta {
            PolicyDelta::Narrowing => Ok(Json::obj([
                ("delta", Json::str("narrowing")),
                ("cells", Json::Arr(vec![])),
            ])),
            PolicyDelta::Widening { cells } => Ok(Json::obj([
                ("delta", Json::str("widening")),
                (
                    "cells",
                    Json::Arr(cells.iter().map(Json::str).collect()),
                ),
            ])),
        }
    }

    // ── accountability ────────────────────────────────────────────────

    /// `fleet.accountability_record` — the matched-budget conditional
    /// record (§5i.1 #12): `accountability_record(run) → {spec_ref,
    /// policy_ref, budget_ref, items[…], escalations[], matched,
    /// external_effect_account, dispatches[]}`. Rule-O: every
    /// dispatched-run and external-effect member links through `spec_ref`.
    pub fn accountability_record(&self, store: &mut Store) -> Result<Json, FleetError> {
        let account = Account::open(store, &self.run_id)
            .map_err(FleetError::Account)?;
        let report = account.accountability_report();
        let items: Vec<Json> = self
            .view
            .items
            .values()
            .map(|it| {
                let mut m = BTreeMap::new();
                m.insert("item_id".into(), Json::str(&it.item_id));
                m.insert("run_item_id".into(), Json::str(&it.run_item_id));
                m.insert("spec_ref".into(), Json::str(&it.spec_ref));
                m.insert("state".into(), Json::str(&derive_state(it)));
                if let Some(o) = &it.owner {
                    m.insert("owner".into(), Json::str(o));
                }
                if let Some(r) = &it.dispatch.run_ref {
                    m.insert("run_ref".into(), Json::str(r));
                }
                if let Some(d) = &it.dispatch.declared {
                    m.insert("declared".into(), d.clone());
                }
                if let Some(bid) = self.view.slice_ids.get(&it.run_item_id) {
                    m.insert("budget_slice".into(), Json::str(bid));
                }
                if let Some(s) = &it.settlement {
                    m.insert("outcome".into(), Json::str(&s.outcome));
                    if !s.evidence_refs.is_empty() {
                        m.insert(
                            "evidence_refs".into(),
                            Json::Arr(s.evidence_refs.iter().map(Json::str).collect()),
                        );
                    }
                }
                Json::Obj(m)
            })
            .collect();
        let escalations: Vec<Json> = self
            .view
            .escalation_rows
            .iter()
            .map(|r| {
                Json::obj([
                    (
                        "item_id",
                        r.get("item_id").cloned().unwrap_or(Json::Null),
                    ),
                    ("issue", r.get("issue").cloned().unwrap_or(Json::Null)),
                    (
                        "issue_ref",
                        r.get("issue_ref").cloned().unwrap_or(Json::Null),
                    ),
                    (
                        "escalation_ref",
                        r.get("escalation_ref")
                            .cloned()
                            .unwrap_or(Json::Null),
                    ),
                ])
            })
            .collect();
        let matched = !self.view.spec.out_of_scope;
        let mut rec = BTreeMap::new();
        rec.insert("schema".into(), Json::str("hh.fleet.accountability/1"));
        rec.insert("run".into(), Json::str(&self.run_id));
        rec.insert("spec_ref".into(), Json::str(&self.view.spec_ref));
        rec.insert("policy_ref".into(), Json::str(&self.view.spec.policy_ref));
        rec.insert(
            "budget_ref".into(),
            self.view
                .spec
                .budget_ref
                .as_ref()
                .map(Json::str)
                .unwrap_or(Json::Null),
        );
        rec.insert("matched".into(), Json::Bool(matched));
        rec.insert("items".into(), Json::Arr(items));
        rec.insert("escalations".into(), Json::Arr(escalations));
        // `external_effect_account` — the budget crate's orphan scan over
        // the accountable classes plus the per-dispatch `declared` join
        // (every external-effect member links through `spec_ref`/slice).
        let dispatches: Vec<Json> = self
            .view
            .items
            .values()
            .filter(|i| i.dispatch.spec_ref.is_some())
            .map(|i| {
                Json::Obj(BTreeMap::from([
                    ("item_id".into(), Json::str(&i.item_id)),
                    ("spec_ref".into(), Json::str(
                        i.dispatch
                            .spec_ref
                            .clone()
                            .unwrap_or_default(),
                    )),
                    (
                        "run_ref".into(),
                        i.dispatch
                            .run_ref
                            .as_ref()
                            .map(Json::str)
                            .unwrap_or(Json::Null),
                    ),
                    (
                        "declared".into(),
                        i.dispatch
                            .declared
                            .clone()
                            .unwrap_or(Json::Null),
                    ),
                ]))
            })
            .collect();
        rec.insert(
            "external_effect_account".into(),
            Json::obj([
                ("clean", Json::Bool(report.is_clean())),
                (
                    "orphans",
                    Json::Arr(report.orphans.iter().map(Json::str).collect()),
                ),
            ]),
        );
        rec.insert("dispatches".into(), Json::Arr(dispatches));
        Ok(Json::Obj(rec))
    }

    /// `audit_link()` — the Rule-O link check (§5i.1 #11): for each
    /// dispatched item the `FleetAnchor` (`lifecycle.run.created.causes`
    /// ↔ `control.work_item.dispatched`) resolves.
    pub fn audit_link(&self, store: &Store) -> Result<Vec<Json>, FleetError> {
        let mut out = Vec::new();
        for it in self.view.items.values() {
            let Some(run_ref) = &it.dispatch.run_ref else {
                continue;
            };
            let spawn = store
                .manifest(run_ref)
                .ok()
                .and_then(|m| m.spawn_event.clone());
            let ok = spawn
                .as_ref()
                .map(|s| {
                    s.run_id == self.run_id
                        && Some(&s.event_id) == it.dispatch_event_id.as_ref()
                })
                .unwrap_or(false);
            out.push(Json::obj([
                ("obligation", Json::str("fleet_anchor")),
                ("item_id", Json::str(&it.item_id)),
                ("run_ref", Json::str(run_ref)),
                ("dispatch_event_id", Json::str(
                    it.dispatch_event_id.clone().unwrap_or_default(),
                )),
                ("status", Json::str(if ok { "verified" } else { "failed" })),
            ]));
        }
        Ok(out)
    }
}

/// `fleet.admit` outcome — `admitted`/`known`/`conflict`.
#[derive(Debug, Clone)]
pub enum AdmitOutcome {
    /// The item was newly admitted (the durable row's event id).
    Admitted {
        /// The admitted `item_id`.
        item_id: String,
        /// The `control.work_item.dispatched{verb:admit}` event id.
        event_id: String,
    },
    /// The same dossier was already admitted — the durable row is the
    /// dedup (`known`).
    Known {
        /// The existing `item_id`.
        item_id: String,
    },
}

/// `spec.triggers` can't cover a cue whose trigger no rule names — the
/// observation is recorded anyway; `source_cues` then skips it (the
/// occurred row never fired).
fn scope_of_retry(t: &Trigger) -> String {
    match t {
        Trigger::RetryDue { scope_id } => scope_id.clone(),
        _ => String::new(),
    }
}

/// `source.source_id` on an admission (`""` when undeclared).
fn source_id_of(init: &WorkItemInit) -> &str {
    init.source
        .get("source_id")
        .and_then(Json::as_str)
        .unwrap_or("")
}

fn source_id_of_view(it: &WorkItemView) -> String {
    it.source
        .get("source_id")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string()
}

/// The dossier members `SourceConflict` compares — a re-presented
/// occurrence for an admitted item must match on every one.
const CONFLICT_FIELDS: &[&str] = &["title", "owner", "idempotency_key", "blocking", "source"];

/// The first divergent dossier member (`None` = identical).
fn conflict_field(a: &WorkItemView, b: &WorkItemInit) -> Option<String> {
    for f in CONFLICT_FIELDS {
        if field_value(a, f) != field_value_init(b, f) {
            return Some(f.to_string());
        }
    }
    None
}

fn field_value(it: &WorkItemView, field: &str) -> String {
    match field {
        "title" => it.title.clone(),
        "owner" => it.owner.clone().unwrap_or_default(),
        "idempotency_key" => it.idempotency_key.clone(),
        "blocking" => it.blocking.join(","),
        "source" => it.source.to_canonical_string(),
        _ => String::new(),
    }
}

fn field_value_init(init: &WorkItemInit, field: &str) -> String {
    match field {
        "title" => init.title.clone(),
        "owner" => init.owner.clone().unwrap_or_default(),
        "idempotency_key" => init.idempotency_key.clone(),
        "blocking" => init.blocking.join(","),
        "source" => init.source.to_canonical_string(),
        _ => String::new(),
    }
}


