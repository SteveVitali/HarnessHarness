//! `FleetView` — the owner's durable projection over the activation run's
//! event prefix (§5i.1 #3's "the fold synthesises …"). Rebuildable at any
//! seq; the `cursor` is the durable-reconcile watermark (`event_count`
//! folded — RC-8's restart-safe resume point).

use hh_ledger::event::EventEnvelope;
use hh_wire::json::Json;
use std::collections::{BTreeMap, BTreeSet};

use crate::errors::FleetError;
use crate::ownership::OwnershipGraph;
use crate::payloads;
use crate::spec::FleetSpec;
use crate::work_item::{
    derive_state, DispatchState, EscalationState, RetryState, Settlement, WorkItemView,
};

/// `Cursor{event_count}` — the reconciler's durable-progress marker. The
/// engine records it on the activation record at every reconcile; a
/// restart resumes the fold from it (never re-deriving a decision).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cursor {
    /// Events folded (the prefix length the view covers).
    pub event_count: usize,
    /// The adapter-side observation watermark (`occurrences(since)`
    /// — the fixture's `observed_at_ms` is the adapter cursor's clock).
    pub observed_at_ms: u64,
}

/// One fired wakeup the reconciler has not consumed yet — the cue queue
/// (§5i.1 #2: "`observed` rows + `wakeup.fired` deliver
/// `source_occurrence` cues").
#[derive(Debug, Clone, PartialEq)]
pub struct FleetCue {
    /// `subscription_id`.
    pub subscription_id: String,
    /// `occurrence_key`.
    pub occurrence_key: String,
    /// The `control.wakeup.occurred` event the fire claimed.
    pub occurred_event: String,
    /// The subscription's trigger (decoded — `external{kind}`,
    /// `manual{principal}`, `timer{at_ms}`, `retry_due{scope_id}`).
    pub trigger: hh_ledger::wakeup::Trigger,
    /// The occurred row's `payload_ref` — for source occurrences, the
    /// `context.observation.recorded` event id carrying the `item` dossier.
    pub payload_ref: Option<String>,
    /// The fired row's event id (the consume-cause).
    pub fired_event_id: String,
    /// `deliver_after` — W-3's withheld-delivery marker.
    pub deliver_after: Option<String>,
}

/// `FleetView` — the folded activation state.
#[derive(Debug, Clone)]
pub struct FleetView {
    /// The activation's `FleetSpec`.
    pub spec: FleetSpec,
    /// `spec_ref = H(canonical(spec))` — the durable spec coordinate.
    pub spec_ref: String,
    /// The `lifecycle.fleet.activated` row's event id.
    pub activated_event_id: String,
    /// `item_id → WorkItemView`.
    pub items: BTreeMap<String, WorkItemView>,
    /// `run_item_id → item_id` (the coordinate index).
    pub by_run_item_id: BTreeMap<String, String>,
    /// `idempotency_key → item_id` (admit dedup).
    pub by_idem_key: BTreeMap<String, String>,
    /// The folded ownership graph (`spec.ownership` + `owner_changed`).
    pub ownership: OwnershipGraph,
    /// `subscription_id → trigger` (the fold's sub→rule join).
    pub sub_triggers: BTreeMap<String, hh_ledger::wakeup::Trigger>,
    /// `subscription_id → occurrence_key → occurred payload` — the
    /// occurred rows the fired rows name (`occurred_event` joins too).
    pub occurred: BTreeMap<String, Json>,
    /// Unconsumed fired cues (fired rows the reconciler hasn't marked —
    /// consumption is a durable decision on the work-item classes; a cue
    /// is "consumed" when its derived decision row exists).
    pub cues: Vec<FleetCue>,
    /// `resolution_count` per item (`lifecycle.escalation.resolved`).
    pub resolution_count: BTreeMap<String, u64>,
    /// `raised_no` per item — the Nth raise seeds `escalation_ref`.
    pub raised_no: BTreeMap<String, u64>,
    /// The activation's root budget id (`control.budget.allocated` root —
    /// `parent` absent), when RC-6's matched arm allocated it.
    pub budget_id: Option<String>,
    /// `run_item_id → slice budget_id` — the per-dispatch matched slices.
    pub slice_ids: BTreeMap<String, String>,
    /// `item_id → ticket_id` — the idem-key→item join is folded; this
    /// member additionally joins `source.ticket_id` for RC-3 conflict keys.
    pub by_ticket: BTreeMap<String, String>,
    /// How many events the fold covers.
    pub event_count: usize,
    /// `suspended` source ids the observation rows declared.
    pub suspended_sources: BTreeSet<String>,
    /// Escalation `raised` rows (the audit trail — newest last).
    pub escalation_rows: Vec<Json>,
    /// The adapter-side observation watermark — `max(observed_at_ms)`
    /// across `context.observation.recorded` rows (the durable cursor's
    /// `since` leg — a restore re-derives it, never persists it).
    pub observed_watermark: u64,
    /// `occurrence_id`s already observed (dedup across `observe` calls —
    /// the adapter replays the whole snapshot; this set is the durable
    /// "already recorded" answer).
    pub observed_occurrence_ids: BTreeSet<String>,
    /// `control.retry.scheduled` event ids consumed by `fired`/`skipped`.
    pub consumed_schedules: BTreeSet<String>,
    /// Live (unconsumed) `control.retry.scheduled` rows —
    /// `schedule_event_id → {scope_id, not_before, attempt_no}`.
    pub pending_schedules: BTreeMap<String, Json>,
    /// `item_id → live scoped lease` (`scope = run_item(X)` — RC-2's
    /// re-fire/restore join; a `released`/`fenced` row removes it).
    pub item_leases: BTreeMap<String, Json>,
    /// `event_id → observation payload` — the `context.observation.recorded`
    /// index the cue→item join resolves through (occurrences name their
    /// observation row by `payload_ref`).
    pub obs_index: BTreeMap<String, Json>,
    /// `control.work_item.annotated` payloads in commit order — durable
    /// annotations (`{subject, subject_ref?, text_ref, readers[],
    /// annotated_by}` — never model-facing; T-LCD-13).
    pub annotations: Vec<Json>,
}

impl FleetView {
    /// `fold(events)` — rebuild the whole projection. The caller passes the
    /// activation run's events; `Err(ActivationNotFound)` when the run is
    /// not a fleet activation.
    pub fn fold(events: &[EventEnvelope]) -> Result<FleetView, FleetError> {
        let mut v: Option<FleetView> = None;
        for e in events {
            if let Some(view) = v.as_mut() {
                view.fold_one(e)?;
            } else if e.class == "lifecycle.fleet.activated" {
                v = Some(FleetView::from_activated(e)?);
            }
        }
        v.ok_or_else(|| FleetError::ActivationNotFound {
            run: events.first().map(|e| e.run_id.clone()).unwrap_or_default(),
        })
    }

    /// `fold_tail(&mut self, events)` — incremental fold from
    /// `event_count` (the engine's restore path folds the tail it hasn't
    /// seen, not the whole prefix).
    pub fn fold_tail(&mut self, events: &[EventEnvelope]) -> Result<(), FleetError> {
        for e in events.iter().skip(self.event_count) {
            self.fold_one(e)?;
        }
        Ok(())
    }

    fn from_activated(e: &EventEnvelope) -> Result<FleetView, FleetError> {
        let (spec, spec_ref) = payloads::spec_from_activated(&e.payload)?;
        Ok(FleetView {
            ownership: OwnershipGraph::from_spec(&spec)?,
            spec,
            spec_ref,
            activated_event_id: e.event_id.clone(),
            items: BTreeMap::new(),
            by_run_item_id: BTreeMap::new(),
            by_idem_key: BTreeMap::new(),
            sub_triggers: BTreeMap::new(),
            occurred: BTreeMap::new(),
            cues: Vec::new(),
            resolution_count: BTreeMap::new(),
            raised_no: BTreeMap::new(),
            budget_id: None,
            slice_ids: BTreeMap::new(),
            by_ticket: BTreeMap::new(),
            event_count: (e.seq + 1) as usize,
            suspended_sources: BTreeSet::new(),
            escalation_rows: Vec::new(),
            observed_watermark: 0,
            observed_occurrence_ids: BTreeSet::new(),
            consumed_schedules: BTreeSet::new(),
            pending_schedules: BTreeMap::new(),
            item_leases: BTreeMap::new(),
            obs_index: BTreeMap::new(),
            annotations: Vec::new(),
        })
    }

    /// One event's fold — every `control.work_item.*`/`lifecycle.escalation.*`/
    /// `control.wakeup.*`/`control.retry.*`/`control.budget.*`/
    /// `context.observation.recorded` member the view consumes.
    fn fold_one(&mut self, e: &EventEnvelope) -> Result<(), FleetError> {
        self.event_count = (e.seq + 1) as usize;
        match e.class.as_str() {
            // `control.work_item.created` — the admission row (§5i.1
            // ADR-0205 D8's `create_work_item → created`; folded by the
            // same `admit` arm as `dispatched{verb:"admit"}` — one shape,
            // two spellings, one fold).
            "control.work_item.created" => self.fold_dispatched(e)?,
            // `control.work_item.annotated` — record-only: annotations are
            // durable on the activation's prefix and never model-facing
            // (T-LCD-13). The fold keeps them for `fleet_view`.
            "control.work_item.annotated" => {
                self.annotations.push(e.payload.clone());
            }
            "control.work_item.dispatched" => self.fold_dispatched(e)?,
            "control.work_item.blocked" => self.fold_blocked(e),
            "control.work_item.stopped" => self.fold_stopped(e),
            "control.work_item.cancelled" => self.fold_cancelled(e),
            "control.work_item.handoff" => self.fold_handoff(e),
            "control.work_item.owner_changed" => self.fold_owner_changed(e),
            "control.work_item.owner_acknowledged" => self.fold_owner_ack(e),
            "lifecycle.escalation.raised" => self.fold_escalation_raised(e),
            "lifecycle.escalation.resolved" => self.fold_escalation_resolved(e),
            "control.wakeup.scheduled" => {
                if let Some(sub) = e.payload.get("subscription") {
                    if let (Some(id), Some(trig)) = (
                        sub.get("subscription_id").and_then(Json::as_str),
                        hh_ledger::wakeup::Trigger::from_json(
                            sub.get("trigger").unwrap_or(&Json::Null),
                        ),
                    ) {
                        self.sub_triggers.insert(id.to_string(), trig);
                    }
                }
            }
            "control.wakeup.occurred" => {
                self.occurred.insert(e.event_id.clone(), e.payload.clone());
            }
            "control.wakeup.fired" => {
                let p = &e.payload;
                let sub_id = p
                    .get("subscription_id")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_string();
                let occurred_event = p
                    .get("occurred_event")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_string();
                let trigger = self.sub_triggers.get(&sub_id).cloned().unwrap_or(
                    hh_ledger::wakeup::Trigger::Manual {
                        principal: "unknown".into(),
                    },
                );
                self.cues.push(FleetCue {
                    subscription_id: sub_id.clone(),
                    occurrence_key: p
                        .get("occurrence_key")
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    occurred_event,
                    trigger,
                    payload_ref: p
                        .get("payload_ref")
                        .and_then(Json::as_str)
                        .map(str::to_string),
                    fired_event_id: e.event_id.clone(),
                    deliver_after: p
                        .get("deliver_after")
                        .and_then(Json::as_str)
                        .map(str::to_string),
                });
            }
            "control.wakeup.skipped" | "control.wakeup.cancelled" => {}
            "context.observation.recorded" => {
                self.obs_index.insert(e.event_id.clone(), e.payload.clone());
                if let Some(s) = e.payload.get("suspended_source").and_then(Json::as_str) {
                    self.suspended_sources.insert(s.to_string());
                }
                if let Some(id) = e
                    .payload
                    .get("trigger")
                    .and_then(|t| t.get("occurrence_id"))
                    .and_then(Json::as_str)
                {
                    self.observed_occurrence_ids.insert(id.to_string());
                }
                if let Some(t) = e.payload.get("observed_at_ms").and_then(Json::as_int) {
                    self.observed_watermark = self.observed_watermark.max(t.max(0) as u64);
                }
            }
            "control.retry.scheduled" => {
                self.pending_schedules
                    .insert(e.event_id.clone(), e.payload.clone());
            }
            "control.retry.fired" | "control.retry.skipped" => {
                // The consume rows fold against the scheduled timers — the
                // view tracks them through `retry.next_attempt_at_ms` on
                // the item itself (the `verb:retry` dispatched row is the
                // item's durable retry state; the retry.* pair is the
                // timer machinery's own accounting).
                if let Some(id) = e.payload.get("schedule_event_id").and_then(Json::as_str) {
                    self.consumed_schedules.insert(id.to_string());
                    self.pending_schedules.remove(id);
                }
            }
            "lifecycle.lease.acquired" | "lifecycle.lease.renewed" => {
                if let Some(item) = e.payload.get("scope").and_then(Json::as_str).and_then(|s| {
                    s.strip_prefix("run_item:")
                        .or_else(|| s.strip_prefix("resource:run_item:"))
                }) {
                    self.item_leases.insert(item.to_string(), e.payload.clone());
                }
            }
            "lifecycle.lease.released" | "lifecycle.lease.fenced" => {
                if let Some(item) = e.payload.get("scope").and_then(Json::as_str).and_then(|s| {
                    s.strip_prefix("run_item:")
                        .or_else(|| s.strip_prefix("resource:run_item:"))
                }) {
                    self.item_leases.remove(item);
                }
            }
            "control.budget.allocated" => {
                let p = &e.payload;
                if p.get("parent").is_none() || p.get("parent") == Some(&Json::Null) {
                    if p.get("outcome").and_then(Json::as_str) != Some("refused") {
                        self.budget_id = p
                            .get("budget_id")
                            .and_then(Json::as_str)
                            .map(str::to_string);
                    }
                } else if let (Some(item), Some(bid)) = (
                    p.get("item_id").and_then(Json::as_str),
                    p.get("budget_id").and_then(Json::as_str),
                ) {
                    if p.get("outcome").and_then(Json::as_str) != Some("refused") {
                        self.slice_ids.insert(item.to_string(), bid.to_string());
                    }
                }
            }
            _ => {}
        }
        // `last_activity` — RC-3's ledger-derived stamp: any folded row
        // naming the item (`item_id`, the retry family's `scope_id`, or a
        // lease row's `scope = run_item:<rid>`) moves its activity stamp.
        let named = payloads::item_id_of(&e.payload)
            .map(str::to_string)
            .or_else(|| {
                e.payload
                    .get("scope_id")
                    .and_then(Json::as_str)
                    .map(str::to_string)
            })
            .or_else(|| {
                e.payload
                    .get("scope")
                    .and_then(Json::as_str)
                    .and_then(|s| {
                        s.strip_prefix("run_item:")
                            .or_else(|| s.strip_prefix("resource:run_item:"))
                            .map(str::to_string)
                    })
                    .and_then(|rid| self.by_run_item_id.get(&rid).cloned())
            });
        if let Some(id) = named {
            if let Some(it) = self.items.get_mut(&id) {
                it.last_activity = e.ts.clone();
            }
        }
        // The derived `state` is recomputed over the member set — never a
        // stored field (the fold derives it; a replay rebuilds identically).
        for it in self.items.values_mut() {
            it.state = derive_state(it);
        }
        Ok(())
    }

    fn fold_dispatched(&mut self, e: &EventEnvelope) -> Result<(), FleetError> {
        let (item_id, verb) = payloads::subject_of(e);
        let Some(item_id) = item_id else {
            return Err(FleetError::InvalidPayload {
                detail: "work_item.dispatched row missing item_id".into(),
            });
        };
        match verb.as_str() {
            "admit" => {
                let init = payloads::init_from_admit(&e.payload)?;
                let run_item_id = crate::identity::run_item_id(&e.run_id, &init.item_id);
                let spec_ref = e
                    .payload
                    .get("spec_ref")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_string();
                let ticket = init
                    .source
                    .get("ticket_id")
                    .and_then(Json::as_str)
                    .map(str::to_string);
                let view = WorkItemView {
                    run_item_id: run_item_id.clone(),
                    item_id: init.item_id.clone(),
                    title: init.title.clone(),
                    source: init.source.clone(),
                    occurrence: init.occurrence.clone(),
                    idempotency_key: init.idempotency_key.clone(),
                    work_item_ref: crate::identity::work_item_ref(&e.run_id, &init.item_id),
                    owner: init.owner.clone(),
                    owner_ack: false,
                    lease_agent_ref: None,
                    blocking: init.blocking.clone(),
                    spec_ref,
                    on: init.on.clone(),
                    dispatch: DispatchState {
                        status: "ready".into(),
                        ..DispatchState::default()
                    },
                    retry: RetryState::default(),
                    escalation: None,
                    settlement: None,
                    blocked: BTreeSet::new(),
                    suspended: false,
                    state: "queued".into(),
                    watch_state: "watching".into(),
                    admitted_event_id: e.event_id.clone(),
                    dispatch_event_id: None,
                    activation_no: 0,
                    last_activity: e.ts.clone(),
                };
                self.by_run_item_id
                    .insert(run_item_id, init.item_id.clone());
                self.by_idem_key
                    .insert(init.idempotency_key.clone(), init.item_id.clone());
                if let Some(t) = ticket {
                    self.by_ticket.insert(t, init.item_id.clone());
                }
                self.items.insert(init.item_id, view);
            }
            "dispatch" => {
                let it = self.item_mut(&item_id)?;
                let d = payloads::dispatch_of(&e.payload);
                it.dispatch.spec_ref = d
                    .and_then(|d| d.get("spec_ref"))
                    .and_then(Json::as_str)
                    .map(str::to_string);
                it.dispatch.lease_ref = d
                    .and_then(|d| d.get("lease_ref"))
                    .and_then(Json::as_str)
                    .map(str::to_string);
                it.dispatch.acquire = d
                    .and_then(|d| d.get("acquire"))
                    .and_then(Json::as_str)
                    .map(str::to_string);
                it.dispatch.started_ts = d
                    .and_then(|d| d.get("started_ts"))
                    .and_then(Json::as_int)
                    .map(|i| i.max(0) as u64);
                it.dispatch.status = "dispatching".into();
                it.dispatch.declared = d.and_then(|d| d.get("declared")).cloned();
                it.dispatch_event_id = Some(e.event_id.clone());
                it.activation_no += 1;
            }
            "run" => {
                let it = self.item_mut(&item_id)?;
                let d = payloads::dispatch_of(&e.payload);
                it.dispatch.run_ref = d
                    .and_then(|d| d.get("run_ref"))
                    .and_then(Json::as_str)
                    .map(str::to_string);
                it.dispatch.status = "dispatched".into();
            }
            "error" => {
                let it = self.item_mut(&item_id)?;
                let d = payloads::dispatch_of(&e.payload);
                it.dispatch.last_error = d
                    .and_then(|d| d.get("last_error"))
                    .and_then(Json::as_str)
                    .map(str::to_string);
                it.dispatch.status = "error".into();
            }
            "retry" => {
                let it = self.item_mut(&item_id)?;
                let r = payloads::retry_of(&e.payload);
                it.retry.spec_ref = r
                    .and_then(|r| r.get("spec_ref"))
                    .and_then(Json::as_str)
                    .map(str::to_string);
                it.retry.attempts = r
                    .and_then(|r| r.get("attempts"))
                    .and_then(Json::as_int)
                    .map(|i| i.max(0) as u64)
                    .unwrap_or(it.retry.attempts);
                it.retry.next_attempt_at_ms = r
                    .and_then(|r| r.get("next_attempt_at_ms"))
                    .and_then(Json::as_int)
                    .map(|i| i.max(0) as u64);
            }
            "source_conflict" => {
                // RC-3 — a conflicting occurrence leaves the item blocked
                // `source_conflict` and records the divergent member.
                let it = self.item_mut(&item_id)?;
                it.blocked.insert("source_conflict".to_string());
            }
            "source_update" => {
                // A source `state` transition — `it.source["state"]` is
                // durable fact the human-gate fold reads (the update row
                // itself is the durable record of the transition).
                let it = self.item_mut(&item_id)?;
                if let Some(state) = e
                    .payload
                    .get("source")
                    .and_then(|s| s.get("state"))
                    .and_then(Json::as_str)
                {
                    let mut src = match it.source.clone() {
                        Json::Obj(m) => m,
                        _ => BTreeMap::new(),
                    };
                    src.insert("state".into(), Json::str(state));
                    it.source = Json::Obj(src);
                }
            }
            "bind_source" => {
                // `bind_source` — the `WorkSourceBinding` lands on the
                // item's `source.bindings[]` (ADR-0205 D5; a PR/thread the
                // agent opens is a `subscription` binding on the SAME
                // item, never a new one).
                let it = self.item_mut(&item_id)?;
                if let Some(b) = e.payload.get("binding").cloned() {
                    let mut src = match it.source.clone() {
                        Json::Obj(m) => m,
                        _ => BTreeMap::new(),
                    };
                    let mut bindings: Vec<Json> = match src.get("bindings") {
                        Some(Json::Arr(a)) => a.clone(),
                        _ => Vec::new(),
                    };
                    // Durable dedup — an identical binding re-presented is
                    // a `known` hit (level-triggered fold).
                    if !bindings.iter().any(|x| x == &b) {
                        bindings.push(b);
                    }
                    src.insert("bindings".into(), Json::Arr(bindings));
                    it.source = Json::Obj(src);
                }
            }
            "resume" => {
                // `resume_from_handoff` — the human act's record; the
                // `human_gate` remove row clears the state (fold_blocked
                // handles the member); this row carries `resumed_by`.
                let it = self.item_mut(&item_id)?;
                it.blocked.remove("human_gate");
            }
            _ => {}
        }
        Ok(())
    }

    fn fold_blocked(&mut self, e: &EventEnvelope) {
        let Some(item_id) = payloads::item_id_of(&e.payload) else {
            return;
        };
        let Ok(it) = self.item_mut(item_id) else {
            return;
        };
        let code = e
            .payload
            .get("code")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        match payloads::verb(&e.payload) {
            "add" => {
                it.blocked.insert(code);
            }
            "remove" => {
                it.blocked.remove(&code);
            }
            _ => {}
        }
    }

    fn fold_stopped(&mut self, e: &EventEnvelope) {
        let Some(item_id) = payloads::item_id_of(&e.payload) else {
            return;
        };
        let Ok(it) = self.item_mut(item_id) else {
            return;
        };
        let verb = payloads::verb(&e.payload);
        match verb {
            "settle" => {
                it.settlement = Some(Settlement {
                    outcome: e
                        .payload
                        .get("outcome")
                        .and_then(Json::as_str)
                        .unwrap_or("completed")
                        .to_string(),
                    evidence_refs: match e.payload.get("evidence_refs") {
                        Some(Json::Arr(a)) => a
                            .iter()
                            .filter_map(Json::as_str)
                            .map(str::to_string)
                            .collect(),
                        _ => Vec::new(),
                    },
                });
                it.watch_state = "dead".into();
            }
            "stop" | "terminal" | "suspended" => {
                it.watch_state = "dead".into();
                if it.settlement.is_none() {
                    it.settlement = Some(Settlement {
                        outcome: e
                            .payload
                            .get("outcome")
                            .and_then(Json::as_str)
                            .unwrap_or("stopped")
                            .to_string(),
                        evidence_refs: Vec::new(),
                    });
                }
            }
            _ => {}
        }
    }

    fn fold_cancelled(&mut self, e: &EventEnvelope) {
        let Some(item_id) = payloads::item_id_of(&e.payload) else {
            return;
        };
        let Ok(it) = self.item_mut(item_id) else {
            return;
        };
        it.watch_state = "dead".into();
        if it.settlement.is_none() {
            it.settlement = Some(Settlement {
                outcome: "cancelled".into(),
                evidence_refs: Vec::new(),
            });
        }
    }

    fn fold_handoff(&mut self, e: &EventEnvelope) {
        let Some(item_id) = payloads::item_id_of(&e.payload) else {
            return;
        };
        let Ok(it) = self.item_mut(item_id) else {
            return;
        };
        it.owner = e
            .payload
            .get("to_agent")
            .and_then(Json::as_str)
            .map(str::to_string);
        it.lease_agent_ref = e
            .payload
            .get("lease_agent_ref")
            .and_then(Json::as_str)
            .map(str::to_string);
        // An ownership handoff is a re-assign — the new owner's ack is
        // pending (the row itself is not an acknowledgement). The
        // `handoff` *state* is a separate concern (the `human_gate` block
        // cause drives it — §5i.1's vocabulary boundary: ownership
        // handoff ≠ handoff state).
        it.owner_ack = false;
    }

    fn fold_owner_changed(&mut self, e: &EventEnvelope) {
        // Two spellings share the class: the item-ownership transfer
        // `{from, to, basis}` (canonical O-4's first event) and the fleet
        // graph edge `{agent, owner}` (`set_owner`). The fold keys on
        // member presence — a transfer row never mints a graph edge.
        if let (Some(_from), Some(to)) = (
            e.payload.get("from").and_then(Json::as_str),
            e.payload.get("to").and_then(Json::as_str),
        ) {
            if let Some(item_id) = payloads::item_id_of(&e.payload) {
                if let Ok(it) = self.item_mut(item_id) {
                    it.owner = Some(to.to_string());
                    // The new owner's ack is pending — dispatch is gated
                    // until `owner_acknowledged{by = to}` lands (O-4).
                    it.owner_ack = false;
                }
            }
            return;
        }
        let agent = e
            .payload
            .get("agent")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        let owner = e
            .payload
            .get("owner")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        if !agent.is_empty() && !owner.is_empty() {
            self.ownership.apply(&agent, &owner);
        }
        // The item-context member is informational — the graph edge is
        // `agent → owner` fleet-wide (§5i.1 #4's per-fleet mapping).
        if let Some(item_id) = payloads::item_id_of(&e.payload) {
            if let Ok(it) = self.item_mut(item_id) {
                if it.owner.as_deref() == Some(agent.as_str()) {
                    it.owner = Some(owner.clone());
                    it.owner_ack = false;
                }
            }
        }
    }

    fn fold_owner_ack(&mut self, e: &EventEnvelope) {
        let Some(item_id) = payloads::item_id_of(&e.payload) else {
            return;
        };
        let Ok(it) = self.item_mut(item_id) else {
            return;
        };
        it.owner_ack = e
            .payload
            .get("ack")
            .map(|a| a == &Json::Bool(true))
            .unwrap_or(true);
        if let Some(l) = e.payload.get("lease_agent_ref").and_then(Json::as_str) {
            it.lease_agent_ref = Some(l.to_string());
        }
    }

    fn fold_escalation_raised(&mut self, e: &EventEnvelope) {
        self.escalation_rows.push(e.payload.clone());
        let p = &e.payload;
        let Some(item_id) = p.get("item_id").and_then(Json::as_str) else {
            return;
        };
        let item_id = item_id.to_string();
        let raised_no = p
            .get("raised_no")
            .and_then(Json::as_int)
            .map(|i| i.max(0) as u64)
            .unwrap_or(1);
        *self.raised_no.entry(item_id.clone()).or_insert(0) = raised_no;
        let handoff = p.get("handoff").cloned();
        if let Ok(it) = self.item_mut(&item_id) {
            it.escalation = Some(EscalationState {
                issue_ref: p
                    .get("issue_ref")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_string(),
                escalation_ref: p
                    .get("escalation_ref")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_string(),
                issue: p
                    .get("issue")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_string(),
                raised_by: p
                    .get("raised_by")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_string(),
                deadline_ms: p
                    .get("deadline_ms")
                    .and_then(Json::as_int)
                    .map(|i| i.max(0) as u64),
                status: "open".into(),
                raised_no,
                handoff,
                cause: p
                    .get("cause")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_string(),
                deadline_sub: p
                    .get("deadline_sub")
                    .and_then(Json::as_str)
                    .map(str::to_string),
            });
            it.blocked.insert("escalation".to_string());
        }
    }

    fn fold_escalation_resolved(&mut self, e: &EventEnvelope) {
        let p = &e.payload;
        let Some(item_id) = p.get("item_id").and_then(Json::as_str) else {
            return;
        };
        let item_id = item_id.to_string();
        *self.resolution_count.entry(item_id.clone()).or_insert(0) += 1;
        if let Ok(it) = self.item_mut(&item_id) {
            it.escalation = None;
            it.blocked.remove("escalation");
        }
    }

    /// `item_mut` — the fold's tolerant accessor (`InvalidPayload` on a
    /// row naming an unadmitted item — the durable prefix can carry an
    /// op row before its admit when a crash landed between stages; the
    /// fold treats that as malformed input, not a missing item).
    fn item_mut(&mut self, item_id: &str) -> Result<&mut WorkItemView, FleetError> {
        self.items
            .get_mut(item_id)
            .ok_or_else(|| FleetError::InvalidPayload {
                detail: format!("work item {item_id} row before admit"),
            })
    }

    /// `item(item_id)` — the read path (`UnknownItem` at the boundary).
    pub fn item(&self, item_id: &str) -> Result<&WorkItemView, FleetError> {
        self.items
            .get(item_id)
            .ok_or_else(|| FleetError::UnknownItem {
                item: item_id.to_string(),
            })
    }

    /// The active dispatch count — RC-6's `activate_run` denominator
    /// (`dispatching` ∪ `dispatched` non-terminal items).
    pub fn active_dispatch_count(&self) -> usize {
        self.items
            .values()
            .filter(|i| {
                matches!(i.dispatch.status.as_str(), "dispatching" | "dispatched")
                    && i.watch_state == "watching"
                    && i.settlement.is_none()
            })
            .count()
    }

    /// The live item count — RC-6's `items` bound.
    pub fn live_item_count(&self) -> usize {
        self.items
            .values()
            .filter(|i| i.watch_state == "watching" && i.settlement.is_none())
            .count()
    }
}
