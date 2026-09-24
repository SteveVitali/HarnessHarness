//! The `R-2.2.3⁰ᵃ` Stage-1 durability slice (ADR-0130; §5a.3) — `restore` per
//! scope, writer takeover, `retry_due` recomputation and the audited
//! `lifecycle.run.resumed` row.
//!
//! Every recovery decision is a **pure function of the durable ledger** — the
//! `checkpoint` view is the fold that names them (AC-R-2.2.3-2); no model call
//! rebuilds a view and nothing is recomputed from process memory. Actions are
//! appended **one durable row at a time** so a crash mid-restore leaves a
//! committed prefix and a re-run is idempotent — every action is gated on the
//! fold's current phase, which already reflects the rows written so far.
//!
//! The recovery table (ADR-0130 §4), as sliced for Stage 1:
//!
//! | durable state | restore action |
//! |---|---|
//! | effect `intended` | keep — the executor continues it |
//! | effect `authorized`, never `prepared` | keep — reported in `effects_to_prepare` |
//! | effect `prepared`/`deferred`, `read_only` | re-prepare in place (`prepared` re-appended) |
//! | effect `prepared`/`deferred`, non-`read_only` | `unknown{cause: worker_lost}` + probe timer |
//! | effect `committed`, no terminal | `unknown{cause: worker_lost}` + probe timer |
//! | effect `unknown` | keep — ensure a probe timer is live |
//! | open `model_call` scope | `model.call.failed{cause: worker_lost}` + retry timer |
//! | pending `security.permission.pending` | keep — reported in `pending_permissions` |
//! | retry schedule | `retry_due(now)` recomputes from `control.retry.*` |
//!
//! Stage-2 `suspend`/wakeup subscriptions and detached reconciliation are
//! out of scope (S2.3 owns them).

use std::collections::{BTreeMap, BTreeSet};

use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::classes::ScopeKind;
use crate::effect::EffectPhase;
use crate::errors::LedgerError;
use crate::event::{Event, Producer, Scope};
use crate::store::{Lease, Store, KERNEL_EFFECT};

/// The retry backoff applied to a `worker_lost` re-dispatch/probe at restore —
/// a Stage-1 constant (the class-policy table refines it with the scheduler).
pub const RESTORE_RETRY_BACKOFF_MS: u64 = 1_000;

/// One recovery action the restore wrote (audit — the rows themselves are the
/// durable record; the report just makes them greppable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreAction {
    /// The appended event.
    pub event_id: String,
    /// Its class.
    pub class: String,
    /// The scope it settles (`effect_id` / `model_call_id`).
    pub subject: String,
}

/// A live retry timer — a `control.retry.scheduled` row no `fired`/`skipped`
/// has consumed (the durable timer; ADR-0130 §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryTimer {
    /// The `control.retry.scheduled` event.
    pub schedule_event_id: String,
    /// The scope the retry re-drives (`effect_id` / `model_call_id`).
    pub scope_id: String,
    /// `model_call` | `probe` | caller-defined.
    pub kind: String,
    /// Earliest fire time (wall-ms).
    pub not_before: u64,
    /// The attempt the retry drives, when recorded.
    pub attempt_no: Option<u64>,
}

/// What `restore` did and found — a report over durable facts, never a
/// substitute for them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    /// The run.
    pub run_id: String,
    /// The new writer generation (the takeover fenced any stale holder).
    pub generation: u64,
    /// The fenced lease id, when the takeover displaced a live holder.
    pub fenced_lease_id: Option<String>,
    /// Recovery rows written, in append order.
    pub actions: Vec<RestoreAction>,
    /// Effects marked `unknown{worker_lost}` by this restore.
    pub unknowned: Vec<String>,
    /// `read_only` effects re-prepared in place.
    reprepared: Vec<String>,
    /// `authorized`-but-never-`prepared` effects the executor must prepare.
    pub effects_to_prepare: Vec<String>,
    /// Model-call scopes failed `worker_lost` and scheduled for retry.
    pub failed_model_calls: Vec<String>,
    /// `security.permission.pending` with no final `decided` — kept pending.
    pub pending_permissions: Vec<String>,
    /// Terminal-`observed` effects whose charge repost is the caller's to check
    /// (the ledger records; `hh-budget` reposts — idempotent by source event).
    pub observed_effects: Vec<String>,
    /// The `lifecycle.run.resumed` audit row.
    pub resumed_event_id: String,
    /// The post-takeover writer lease — recovery and the resuming writer share
    /// one generation, so its `fencing_token`s and the token check agree.
    #[doc(hidden)]
    pub lease: Lease,
}

impl RestoreReport {
    /// `read_only` effects re-prepared in place.
    pub fn reprepared(&self) -> &[String] {
        &self.reprepared
    }
}

/// The restore scratch — accumulates the durable actions and the report fields
/// so the step helpers stay under the arity lint without a tuple soup.
struct RestoreOut {
    actions: Vec<RestoreAction>,
    unknowned: Vec<String>,
    reprepared: Vec<String>,
    effects_to_prepare: Vec<String>,
    failed_model_calls: Vec<String>,
    pending_permissions: Vec<String>,
    observed_effects: Vec<String>,
}

/// Mint a kernel-authored `Event` (unstaged — `store.append` stamps it). Shared
/// by the recovery/lease/suspend/wakeup/saga emitters (one kernel producer —
/// CC1).
pub(crate) fn kernel_ev(
    store: &Store,
    run_id: &str,
    class: &str,
    scope: Scope,
    payload: Json,
) -> Result<Event, LedgerError> {
    Ok(Event {
        event_id: store.alloc_id("evt"),
        class: class.to_string(),
        ts: store.ts_now(),
        hlc: None,
        producer: Producer::kernel(KERNEL_EFFECT),
        scope,
        parent_event_id: store.head_event_id(run_id)?,
        causes: Vec::new(),
        refs: Vec::new(),
        ir_refs: Vec::new(),
        surface_ids: BTreeMap::new(),
        provenance: Some(ProvenanceRecord::kernel(KERNEL_EFFECT, store.now_ms())),
        content_kind: None,
        payload,
    })
}

/// Does an open retry timer already cover `(scope_id, kind)`?
fn timer_live(events: &[crate::event::EventEnvelope], scope_id: &str, kind: &str) -> bool {
    let mut scheduled: BTreeMap<String, &crate::event::EventEnvelope> = BTreeMap::new();
    for e in events {
        match e.class.as_str() {
            "control.retry.scheduled" => {
                scheduled.insert(e.event_id.clone(), e);
            }
            "control.retry.fired" | "control.retry.skipped" => {
                if let Some(id) = e.payload.get("schedule_event_id").and_then(Json::as_str) {
                    scheduled.remove(id);
                }
            }
            _ => {}
        }
    }
    scheduled.values().any(|e| {
        e.payload.get("scope_id").and_then(Json::as_str) == Some(scope_id)
            && e.payload.get("kind").and_then(Json::as_str) == Some(kind)
    })
}

impl Store {
    /// `retry_due(run, now) → [RetryTimer]` — the `control.retry.scheduled` rows
    /// not yet consumed (`fired`/`skipped`) whose `not_before ≤ now`. Pure fold
    /// of the durable prefix — a restart recomputes the same set (AC-R-2.2.3-2).
    pub fn retry_due(&self, run_id: &str, now_ms: u64) -> Result<Vec<RetryTimer>, LedgerError> {
        let events = self.events(run_id)?;
        let mut scheduled: BTreeMap<String, RetryTimer> = BTreeMap::new();
        for e in events {
            match e.class.as_str() {
                "control.retry.scheduled" => {
                    scheduled.insert(
                        e.event_id.clone(),
                        RetryTimer {
                            schedule_event_id: e.event_id.clone(),
                            scope_id: e
                                .payload
                                .get("scope_id")
                                .and_then(Json::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            kind: e
                                .payload
                                .get("kind")
                                .and_then(Json::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            not_before: e
                                .payload
                                .get("not_before")
                                .and_then(Json::as_int)
                                .unwrap_or(0)
                                .max(0) as u64,
                            attempt_no: e
                                .payload
                                .get("attempt_no")
                                .and_then(Json::as_int)
                                .map(|n| n.max(0) as u64),
                        },
                    );
                }
                "control.retry.fired" | "control.retry.skipped" => {
                    if let Some(id) = e.payload.get("schedule_event_id").and_then(Json::as_str) {
                        scheduled.remove(id);
                    }
                }
                _ => {}
            }
        }
        Ok(scheduled
            .into_values()
            .filter(|t| t.not_before <= now_ms)
            .collect())
    }

    /// `restore(run, holder, ttl)` — the `R-2.2.3⁰ᵃ` procedure: take the writer
    /// lease (fencing and auditing any stale holder), walk the durable fold per
    /// the ADR-0130 §4 table, append each recovery row individually, then the
    /// `lifecycle.run.resumed` audit row.
    ///
    /// Idempotent: a crashed restore is re-run wholesale — every action checks
    /// the fold's *current* phase, which already reflects what the torn restore
    /// committed.
    pub fn restore(
        &mut self,
        run_id: &str,
        holder: &str,
        ttl_ms: u64,
    ) -> Result<RestoreReport, LedgerError> {
        self.restore_caused(run_id, holder, ttl_ms, "crash")
    }

    /// `restore` with the §5a.3 `cause ∈ {crash, takeover, wakeup, operator,
    /// continuation}` — the `lifecycle.run.resumed{recovery_decision{…, cause}}`
    /// member the cue reports (S2.3).
    pub fn restore_caused(
        &mut self,
        run_id: &str,
        holder: &str,
        ttl_ms: u64,
        cause: &str,
    ) -> Result<RestoreReport, LedgerError> {
        let lease = self.acquire_writer(holder, run_id, ttl_ms)?;
        let now = self.now_ms();
        let gen = lease.generation;
        // `last_durable_phase` — the head's class before any recovery row
        // (the `recovery_decision` member the spec names).
        let (last_durable_phase, from_seq) = {
            let st = self.run(run_id)?;
            match st.head.as_ref() {
                Some(h) => (
                    st.events
                        .get(h.seq as usize)
                        .map(|e| e.class.clone())
                        .unwrap_or_else(|| "lifecycle.run.created".to_string()),
                    h.seq,
                ),
                None => ("lifecycle.run.created".to_string(), 0),
            }
        };
        let was_suspended = self.run(run_id)?.suspended;
        let fenced_lease_id = {
            // The takeover row names the stale lease; recover it from the audit.
            let st = self.run(run_id)?;
            st.events
                .iter()
                .rev()
                .find(|e| e.class == "lifecycle.lease.fenced")
                .and_then(|e| {
                    e.payload
                        .get("stale_lease_id")
                        .and_then(Json::as_str)
                        .map(str::to_string)
                })
        };

        let mut out = RestoreOut {
            actions: Vec::new(),
            unknowned: Vec::new(),
            reprepared: Vec::new(),
            effects_to_prepare: Vec::new(),
            failed_model_calls: Vec::new(),
            pending_permissions: Vec::new(),
            observed_effects: Vec::new(),
        };

        // ── effects (ADR-0130 §4 effect rows) ───────────────────────────
        let effect_ids: Vec<String> = self.run(run_id)?.effects.keys().cloned().collect();
        for effect_id in effect_ids {
            let f = self.run(run_id)?.effects[&effect_id].clone();
            match f.phase {
                EffectPhase::Prepared | EffectPhase::Deferred => {
                    if f.risk_class.is_read_only() {
                        // KP-13 idempotence: a `prepared{reprepared_after}`
                        // row already durable means this restore's decision
                        // was taken by a torn predecessor — do not re-mint
                        // (a second pass completes identically, never twice).
                        let already = self.run(run_id)?.events.iter().any(|e| {
                            e.class == "action.effect.prepared"
                                && e.scope.effect_id.as_deref() == Some(effect_id.as_str())
                                && e.payload
                                    .get("reprepared_after")
                                    .and_then(Json::as_str)
                                    .is_some()
                        });
                        if already {
                            continue;
                        }
                        // Re-prepare — a read_only attempt never wrote ahead.
                        let key = f.idempotency_key.clone().ok_or_else(|| {
                            LedgerError::SchemaViolation {
                                detail: format!(
                                    "effect {effect_id} is prepared without an \
                                     idempotency_key — cannot re-prepare"
                                ),
                            }
                        })?;
                        // Scope to the members still open — a second restore
                        // after a torn first pass finds the model_call already
                        // `failed` closed (KP-13 idempotence); minting under a
                        // dead scope is a `ScopeNotOpen` self-inflicted crash.
                        // The chain rule (`tool_call ⇒ model_call`) drops the
                        // child when the parent is gone.
                        let open = self.run(run_id)?.open_scopes.clone();
                        let live = |k: ScopeKind, v: &Option<String>| {
                            v.as_ref().filter(|id| open.get(*id) == Some(&k)).cloned()
                        };
                        let mut rep_scope = Scope {
                            turn_id: live(ScopeKind::Turn, &f.turn_id),
                            model_call_id: live(ScopeKind::ModelCall, &f.model_call_id),
                            tool_call_id: live(ScopeKind::ToolCall, &f.tool_call_id),
                            effect_id: live(ScopeKind::Effect, &Some(effect_id.clone())),
                            child_run_id: None,
                            component_call_id: None,
                            branch_id: None,
                        };
                        if rep_scope.model_call_id.is_none() {
                            rep_scope.tool_call_id = None;
                        }
                        let ev = kernel_ev(
                            self,
                            run_id,
                            "action.effect.prepared",
                            rep_scope,
                            Json::obj([
                                ("idempotency_key", Json::str(key)),
                                ("reprepared_after", Json::str("worker_lost")),
                            ]),
                        )?;
                        let id = ev.event_id.clone();
                        self.append_recovery(run_id, &lease, vec![ev])?;
                        out.actions.push(RestoreAction {
                            event_id: id,
                            class: "action.effect.prepared".into(),
                            subject: effect_id.clone(),
                        });
                        out.reprepared.push(effect_id);
                    } else {
                        self.restore_mark_unknown(run_id, &lease, &f, gen, now, &mut out)?;
                    }
                }
                EffectPhase::Committed => {
                    self.restore_mark_unknown(run_id, &lease, &f, gen, now, &mut out)?;
                }
                EffectPhase::Unknown => {
                    // Already explicit — ensure a probe timer is live.
                    self.restore_probe_timer(
                        run_id,
                        &lease,
                        &effect_id,
                        f.attempt_no,
                        now,
                        &mut out,
                    )?;
                }
                EffectPhase::Observed => {
                    if f.is_terminal() {
                        out.observed_effects.push(effect_id);
                    } else {
                        // Non-terminal `observed` — `partial` (settlement
                        // pending) or a retryable `not_applied` whose retry was
                        // lost: unresolved ⇒ fence it `unknown`; the probe
                        // decides the rest (ADR-0238 §1).
                        self.restore_mark_unknown(run_id, &lease, &f, gen, now, &mut out)?;
                    }
                }
                EffectPhase::Authorized => {
                    out.effects_to_prepare.push(effect_id);
                }
                _ => {} // intended keeps; terminals are settled.
            }
        }

        // ── open model calls — fail worker-lost + schedule retry ────────
        let open_mcs: Vec<String> = self
            .run(run_id)?
            .open_scopes
            .iter()
            .filter(|(_, k)| **k == crate::classes::ScopeKind::ModelCall)
            .map(|(id, _)| id.clone())
            .collect();
        for mc in open_mcs {
            // The turn scope comes from the `model.call.requested` opener row.
            let turn = self
                .run(run_id)?
                .events
                .iter()
                .find(|e| {
                    e.class == "model.call.requested"
                        && e.scope.model_call_id.as_deref() == Some(mc.as_str())
                })
                .and_then(|e| e.scope.turn_id.clone());
            let ev = kernel_ev(
                self,
                run_id,
                "model.call.failed",
                Scope {
                    turn_id: turn.clone(),
                    model_call_id: Some(mc.clone()),
                    tool_call_id: None,
                    effect_id: None,
                    child_run_id: None,
                    component_call_id: None,
                    branch_id: None,
                },
                Json::obj([("cause", Json::str("worker_lost"))]),
            )?;
            let id = ev.event_id.clone();
            self.append_recovery(run_id, &lease, vec![ev])?;
            out.actions.push(RestoreAction {
                event_id: id,
                class: "model.call.failed".into(),
                subject: mc.clone(),
            });
            out.failed_model_calls.push(mc.clone());
            self.restore_schedule(
                run_id,
                &lease,
                &mc,
                "model_call",
                None,
                now + RESTORE_RETRY_BACKOFF_MS,
                "worker_lost",
                &mut out,
            )?;
        }

        // ── pending permissions — keep (refuse-on-timeout is §05g's) ────
        // The durable owed-decision row is `security.permission.pending`
        // (S1.23 — `requested` is the ephemeral prompt *rendering*, never the
        // owed-decision source). A pending resolves when a `decided` row lands
        // for its `permission_id`, or when every attached `effect_id` reached a
        // refusal/terminal (the `timed_out`/`cancelled` refusal — a pending
        // never ends `unknown`; §5g.7 §5).
        {
            let st = self.run(run_id)?;
            let mut decided = BTreeSet::new();
            let mut terminated_effects = BTreeSet::new();
            // `permission_id → attached effect_ids` — coalesced pendings
            // arrive as duplicate `pending` rows with the same id and merge
            // (the identical-request rule, §5g.7 §5).
            let mut pendings: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            for e in &st.events {
                match e.class.as_str() {
                    "security.permission.decided" => {
                        // Only a *final* verdict resolves a pending — a
                        // `decision = ask` row is the verdict that opened it.
                        let final_verdict = e
                            .payload
                            .get("decision")
                            .and_then(Json::as_str)
                            .is_some_and(|d| d == "allow" || d == "deny");
                        if final_verdict {
                            if let Some(id) = e.payload.get("permission_id").and_then(Json::as_str)
                            {
                                decided.insert(id.to_string());
                            }
                        }
                    }
                    "action.effect.refused" | "action.effect.unknown" => {
                        if let Some(id) = e
                            .scope
                            .effect_id
                            .as_deref()
                            .or_else(|| e.payload.get("effect_id").and_then(Json::as_str))
                        {
                            terminated_effects.insert(id.to_string());
                        }
                    }
                    "security.permission.pending" => {
                        if let Some(id) = e.payload.get("permission_id").and_then(Json::as_str) {
                            // The attached effects — `effect_ids[]` when the
                            // row coalesced, `effect_id`/`scope.effect_id`
                            // for the single-effect form.
                            let mut effects: BTreeSet<String> = e
                                .payload
                                .get("effect_ids")
                                .and_then(|v| match v {
                                    Json::Arr(rows) => Some(
                                        rows.iter()
                                            .filter_map(Json::as_str)
                                            .map(str::to_string)
                                            .collect(),
                                    ),
                                    _ => None,
                                })
                                .unwrap_or_default();
                            if let Some(one) = e
                                .scope
                                .effect_id
                                .as_deref()
                                .or_else(|| e.payload.get("effect_id").and_then(Json::as_str))
                            {
                                effects.insert(one.to_string());
                            }
                            pendings.entry(id.to_string()).or_default().extend(effects);
                        }
                    }
                    _ => {}
                }
            }
            for (id, effects) in pendings {
                if decided.contains(&id) {
                    continue;
                }
                // Every attached effect refused/unknowned → the pending is
                // `cancelled`, never still-owed.
                if !effects.is_empty() && effects.iter().all(|e| terminated_effects.contains(e)) {
                    continue;
                }
                out.pending_permissions.push(id);
            }
        }

        // ── the audited resume row ──────────────────────────────────────
        // `recovery_decision{last_durable_phase, action, cause}` — the §5a.3
        // `Cue.resumed` payload: `action ∈ {continue, resolve_unknowns,
        // suspend, stop{reason}}` — `resolve_unknowns` when the table fenced
        // effects to `unknown`/kept probes, `continue` otherwise. A suspended
        // run resumed here reports `continue` (the suspension's end IS this
        // restore — the `resumed` row clears the `suspended` fold).
        let action = if !out.unknowned.is_empty() || !out.failed_model_calls.is_empty() {
            "resolve_unknowns"
        } else {
            "continue"
        };
        let ev = kernel_ev(
            self,
            run_id,
            "lifecycle.run.resumed",
            Scope::default(),
            Json::obj([
                ("from_seq", Json::Int(from_seq as i64)),
                ("holder", Json::str(holder)),
                ("generation", Json::Int(gen as i64)),
                // `resumed_at_ms` — the `Cue.resumed` delivery timestamp
                // (the `recovery_latency_ms` metric's end anchor —
                // AC-R-2.2.3-12).
                ("resumed_at_ms", Json::Int(self.now_ms() as i64)),
                ("actions", Json::Int(out.actions.len() as i64)),
                (
                    "recovery_decision",
                    Json::obj([
                        ("last_durable_phase", Json::str(&last_durable_phase)),
                        ("action", Json::str(action)),
                        ("cause", Json::str(cause)),
                        ("was_suspended", Json::Bool(was_suspended)),
                    ]),
                ),
            ]),
        )?;
        let resumed_event_id = ev.event_id.clone();
        self.append_recovery(run_id, &lease, vec![ev])?;

        Ok(RestoreReport {
            run_id: run_id.to_string(),
            generation: gen,
            fenced_lease_id,
            actions: out.actions,
            unknowned: out.unknowned,
            reprepared: out.reprepared,
            effects_to_prepare: out.effects_to_prepare,
            failed_model_calls: out.failed_model_calls,
            pending_permissions: out.pending_permissions,
            observed_effects: out.observed_effects,
            resumed_event_id,
            lease,
        })
    }

    /// `unknown{cause: worker_lost}` + a probe timer — the committed/`prepared`
    /// non-`read_only` recovery step.
    fn restore_mark_unknown(
        &mut self,
        run_id: &str,
        lease: &Lease,
        f: &crate::effect::EffectFold,
        gen: u64,
        now: u64,
        out: &mut RestoreOut,
    ) -> Result<(), LedgerError> {
        let ev = kernel_ev(
            self,
            run_id,
            "action.effect.unknown",
            Scope {
                turn_id: f.turn_id.clone(),
                model_call_id: f.model_call_id.clone(),
                tool_call_id: f.tool_call_id.clone(),
                effect_id: Some(f.effect_id.clone()),
                child_run_id: None,
                component_call_id: None,
                branch_id: None,
            },
            Json::obj([
                ("cause", Json::str("worker_lost")),
                ("attempt_no", Json::Int(f.attempt_no as i64)),
                ("fencing_token", Json::Int(gen as i64)),
            ]),
        )?;
        let id = ev.event_id.clone();
        self.append_recovery(run_id, lease, vec![ev])?;
        out.actions.push(RestoreAction {
            event_id: id,
            class: "action.effect.unknown".into(),
            subject: f.effect_id.clone(),
        });
        out.unknowned.push(f.effect_id.clone());
        self.restore_probe_timer(run_id, lease, &f.effect_id, f.attempt_no, now, out)
    }

    /// Append a probe retry timer for an `unknown` effect unless one is live.
    fn restore_probe_timer(
        &mut self,
        run_id: &str,
        lease: &Lease,
        effect_id: &str,
        attempt_no: u64,
        now: u64,
        out: &mut RestoreOut,
    ) -> Result<(), LedgerError> {
        if timer_live(self.events(run_id)?, effect_id, "probe") {
            return Ok(());
        }
        self.restore_schedule(
            run_id,
            lease,
            effect_id,
            "probe",
            Some(attempt_no),
            now + RESTORE_RETRY_BACKOFF_MS,
            "worker_lost",
            out,
        )
    }

    /// One `control.retry.scheduled` row — the durable timer (ADR-0130 §5).
    #[allow(clippy::too_many_arguments)]
    fn restore_schedule(
        &mut self,
        run_id: &str,
        lease: &Lease,
        scope_id: &str,
        kind: &str,
        attempt_no: Option<u64>,
        not_before: u64,
        reason: &str,
        out: &mut RestoreOut,
    ) -> Result<(), LedgerError> {
        let mut payload = BTreeMap::from([
            ("scope_id".to_string(), Json::str(scope_id)),
            ("kind".to_string(), Json::str(kind)),
            ("not_before".to_string(), Json::Int(not_before as i64)),
            ("reason".to_string(), Json::str(reason)),
        ]);
        if let Some(n) = attempt_no {
            payload.insert("attempt_no".to_string(), Json::Int(n as i64));
        }
        let ev = kernel_ev(
            self,
            run_id,
            "control.retry.scheduled",
            Scope::default(),
            Json::Obj(payload),
        )?;
        let id = ev.event_id.clone();
        self.append_recovery(run_id, lease, vec![ev])?;
        out.actions.push(RestoreAction {
            event_id: id,
            class: "control.retry.scheduled".into(),
            subject: scope_id.to_string(),
        });
        Ok(())
    }
}
