//! Durable wakeups (§5a.3; ADR-0131 §4 — `R-2.2.3` C1·S2): the kernel-internal
//! wakeup table — `control.wakeup.{scheduled,occurred,fired,skipped,cancelled}`
//! rows, the `WakeupSubscription` fold, and the five Stage-2 kernel-internal
//! triggers (`timer`, `permission_decided`, `child_terminal`, `effect_terminal`,
//! `retry_due`).
//!
//! The discipline (W-1…W-5): `occurred` is durable **before** any `fired` (a
//! crash between them — KP-11 — leaves a pending occurrence the next
//! [`Store::deliver_wakeup`] picks up); `fire` acquires the
//! `wakeup(subscription, occurrence)` claim lease and appends `fired` in **one
//! atomic batch**, so a duplicate delivery or a racing resumer is `skipped{
//! already_claimed}` — at most one fire per `(subscription, occurrence)`;
//! every skip reason (`expired`, `subscription_cancelled`, `run_finished`,
//! `duplicate_occurrence`, `already_claimed`, `coalesced`, `over_max_pending`)
//! is audited as `control.wakeup.skipped`; `steer` delivery is refused — the
//! OQ-316 ratified default admits `follow_up` only (ADR-0132).
//!
//! `deliver_after` (W-3): a `follow_up` fire while an effect is `committed`
//! records `deliver_after = effect_id` — [`Store::wakeup_drain`] withholds it
//! until the effect reaches a terminal, so the `Cue.woken` lands at a decision
//! point, never mid-effect.
//!
//! Stage-4+ triggers (`schedule`, `external`, `manual`, `peer_message`,
//! `environment_ready`) parse but are refused `TriggerUnsupported` at
//! `subscribe` — honest, never silently swallowed (ADR-0131 §4 stage table).

use std::collections::BTreeMap;

use hh_wire::json::Json;

use crate::errors::LedgerError;
use crate::event::Scope;
use crate::leases::LeaseScope;
use crate::manifest::EventRef;
use crate::recovery::kernel_ev;
use crate::store::{Lease, Store};

/// The maximum live subscriptions per run (a kernel bound — the run's wakeup
/// surface is a resource; `SubscriptionLimit` is the refusal).
pub const MAX_WAKEUP_SUBSCRIPTIONS: usize = 64;

/// `Trigger` — the closed sum (§5a.3 wakeup row). The Stage-2 admissible set is
/// the kernel-internal five; the rest parse and refuse typed at `subscribe`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    /// A one-shot wall-clock timer (`{type: timer, at_ms}`).
    Timer {
        /// Fire at/after this wall-ms.
        at_ms: u64,
    },
    /// A cron-like schedule (Stage 4 — refused at subscribe).
    Schedule {
        /// The schedule expression.
        expr: String,
    },
    /// A `security.permission.decided` for `permission_id` (the §05g `defer`
    /// resolution path).
    PermissionDecided {
        /// The permission the subscription waits on.
        permission_id: String,
    },
    /// The named child run reaching `lifecycle.run.finished`.
    ChildTerminal {
        /// The child.
        child_run_id: String,
    },
    /// The named effect reaching a terminal phase.
    EffectTerminal {
        /// The effect.
        effect_id: String,
    },
    /// The environment reporting ready (Stage 4 boundary — refused at
    /// subscribe at this stage).
    EnvironmentReady {
        /// The handle.
        env_handle_id: String,
    },
    /// A `control.retry.*` timer for `scope_id` coming due.
    RetryDue {
        /// The scope the retry drives.
        scope_id: String,
    },
    /// An external ingress occurrence (Stage 4).
    External {
        /// The ingress kind.
        kind: String,
    },
    /// A principal-initiated wakeup (Stage 4 — `resume` is its attended form).
    Manual {
        /// The principal.
        principal: String,
    },
    /// A peer-run message (Stage 4 — R-2.6.5).
    PeerMessage {
        /// The peer.
        from: String,
    },
}

impl Trigger {
    /// The canonical `{type, …}` payload form.
    pub fn to_json(&self) -> Json {
        match self {
            Trigger::Timer { at_ms } => Json::obj([
                ("type", Json::str("timer")),
                ("at_ms", Json::Int(*at_ms as i64)),
            ]),
            Trigger::Schedule { expr } => {
                Json::obj([("type", Json::str("schedule")), ("expr", Json::str(expr))])
            }
            Trigger::PermissionDecided { permission_id } => Json::obj([
                ("type", Json::str("permission_decided")),
                ("permission_id", Json::str(permission_id)),
            ]),
            Trigger::ChildTerminal { child_run_id } => Json::obj([
                ("type", Json::str("child_terminal")),
                ("child_run_id", Json::str(child_run_id)),
            ]),
            Trigger::EffectTerminal { effect_id } => Json::obj([
                ("type", Json::str("effect_terminal")),
                ("effect_id", Json::str(effect_id)),
            ]),
            Trigger::EnvironmentReady { env_handle_id } => Json::obj([
                ("type", Json::str("environment_ready")),
                ("env_handle_id", Json::str(env_handle_id)),
            ]),
            Trigger::RetryDue { scope_id } => Json::obj([
                ("type", Json::str("retry_due")),
                ("scope_id", Json::str(scope_id)),
            ]),
            Trigger::External { kind } => {
                Json::obj([("type", Json::str("external")), ("kind", Json::str(kind))])
            }
            Trigger::Manual { principal } => Json::obj([
                ("type", Json::str("manual")),
                ("principal", Json::str(principal)),
            ]),
            Trigger::PeerMessage { from } => Json::obj([
                ("type", Json::str("peer_message")),
                ("from", Json::str(from)),
            ]),
        }
    }

    /// Parse the canonical form (`None` on an unknown `type`).
    pub fn from_json(j: &Json) -> Option<Trigger> {
        let s = |k: &str| j.get(k).and_then(Json::as_str).map(str::to_string);
        Some(match j.get("type")?.as_str()? {
            "timer" => Trigger::Timer {
                at_ms: j.get("at_ms")?.as_int()?.max(0) as u64,
            },
            "schedule" => Trigger::Schedule { expr: s("expr")? },
            "permission_decided" => Trigger::PermissionDecided {
                permission_id: s("permission_id")?,
            },
            "child_terminal" => Trigger::ChildTerminal {
                child_run_id: s("child_run_id")?,
            },
            "effect_terminal" => Trigger::EffectTerminal {
                effect_id: s("effect_id")?,
            },
            "environment_ready" => Trigger::EnvironmentReady {
                env_handle_id: s("env_handle_id")?,
            },
            "retry_due" => Trigger::RetryDue {
                scope_id: s("scope_id")?,
            },
            "external" => Trigger::External { kind: s("kind")? },
            "manual" => Trigger::Manual {
                principal: s("principal")?,
            },
            "peer_message" => Trigger::PeerMessage { from: s("from")? },
            _ => return None,
        })
    }

    /// The trigger-type spelling (for refusals and rows).
    pub fn type_name(&self) -> &'static str {
        match self {
            Trigger::Timer { .. } => "timer",
            Trigger::Schedule { .. } => "schedule",
            Trigger::PermissionDecided { .. } => "permission_decided",
            Trigger::ChildTerminal { .. } => "child_terminal",
            Trigger::EffectTerminal { .. } => "effect_terminal",
            Trigger::EnvironmentReady { .. } => "environment_ready",
            Trigger::RetryDue { .. } => "retry_due",
            Trigger::External { .. } => "external",
            Trigger::Manual { .. } => "manual",
            Trigger::PeerMessage { .. } => "peer_message",
        }
    }

    /// Whether the trigger is admissible at this stage — the kernel-internal
    /// five plus `peer_message` (S4.6: the one peer-messaging mechanism —
    /// ADR-0191 D6–D8; `send_message` records an occurrence on the receiver's
    /// `peer_message` subscription). `schedule`/`external`/`manual`/`healed`
    /// still parse and refuse typed at `subscribe`.
    pub fn admissible(&self) -> bool {
        matches!(
            self,
            Trigger::Timer { .. }
                | Trigger::PermissionDecided { .. }
                | Trigger::ChildTerminal { .. }
                | Trigger::EffectTerminal { .. }
                | Trigger::RetryDue { .. }
                | Trigger::PeerMessage { .. }
        )
    }
}

/// `DeliveryMode` — `follow_up` is the only admissible value at this stage
/// (OQ-316's ratified default; `steer` refuses at `subscribe`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryMode {
    /// Delivered at the next decision point, never mid-effect.
    FollowUp,
    /// Steer delivery (deferred — the Stage-4 surface).
    Steer,
}

impl DeliveryMode {
    /// Canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DeliveryMode::FollowUp => "follow_up",
            DeliveryMode::Steer => "steer",
        }
    }

    /// Parse the spelling.
    pub fn parse(s: &str) -> Option<DeliveryMode> {
        match s {
            "follow_up" => Some(DeliveryMode::FollowUp),
            "steer" => Some(DeliveryMode::Steer),
            _ => None,
        }
    }
}

/// `Coalesce` — how multiple pending occurrences of one subscription collapse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coalesce {
    /// Every occurrence fires individually (up to `max_pending`).
    None,
    /// Only the newest pending occurrence fires; the rest are
    /// `skipped{coalesced}`.
    Latest,
    /// All occurrences collapse into one fire (the newest wins; the fold keeps
    /// the full audit trail).
    All,
}

impl Coalesce {
    /// Canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Coalesce::None => "none",
            Coalesce::Latest => "latest",
            Coalesce::All => "all",
        }
    }

    /// Parse the spelling.
    pub fn parse(s: &str) -> Option<Coalesce> {
        match s {
            "none" => Some(Coalesce::None),
            "latest" => Some(Coalesce::Latest),
            "all" => Some(Coalesce::All),
            _ => None,
        }
    }
}

/// `WakeupPolicy{delivery_mode, coalesce, max_pending, expires_at,
/// attendance_required, occurrence_key_fn}` (§5a.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WakeupPolicy {
    /// The delivery mode (`steer` is refused at `subscribe` at this stage).
    pub delivery_mode: DeliveryMode,
    /// The coalescing rule.
    pub coalesce: Coalesce,
    /// The per-subscription pending-occurrence bound.
    pub max_pending: u64,
    /// Subscription expiry (wall-ms; `None` = run lifetime).
    pub expires_at_ms: Option<u64>,
    /// The attendance the delivery requires (`None` = the run's declared
    /// attendance).
    pub attendance_required: Option<String>,
    /// The key function (`None` = the trigger's deterministic key).
    pub occurrence_key_fn: Option<String>,
}

impl WakeupPolicy {
    /// The Stage-2 default — `follow_up`, no coalescing, bounded.
    pub fn default_policy() -> WakeupPolicy {
        WakeupPolicy {
            delivery_mode: DeliveryMode::FollowUp,
            coalesce: Coalesce::None,
            max_pending: 16,
            expires_at_ms: None,
            attendance_required: None,
            occurrence_key_fn: None,
        }
    }

    /// The canonical JSON form.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("delivery_mode", Json::str(self.delivery_mode.as_str())),
            ("coalesce", Json::str(self.coalesce.as_str())),
            ("max_pending", Json::Int(self.max_pending as i64)),
            (
                "expires_at_ms",
                match self.expires_at_ms {
                    Some(t) => Json::Int(t as i64),
                    None => Json::Null,
                },
            ),
            (
                "attendance_required",
                match &self.attendance_required {
                    Some(a) => Json::str(a.clone()),
                    None => Json::Null,
                },
            ),
            (
                "occurrence_key_fn",
                match &self.occurrence_key_fn {
                    Some(k) => Json::str(k.clone()),
                    None => Json::Null,
                },
            ),
        ])
    }

    /// Parse the canonical form (`None` on a bad member).
    pub fn from_json(j: &Json) -> Option<WakeupPolicy> {
        Some(WakeupPolicy {
            delivery_mode: DeliveryMode::parse(
                j.get("delivery_mode")?.as_str().unwrap_or("follow_up"),
            )?,
            coalesce: Coalesce::parse(j.get("coalesce")?.as_str().unwrap_or("none"))?,
            max_pending: j
                .get("max_pending")
                .and_then(Json::as_int)
                .map(|n| n.max(1) as u64)
                .unwrap_or(16),
            expires_at_ms: j
                .get("expires_at_ms")
                .and_then(Json::as_int)
                .map(|n| n.max(0) as u64),
            attendance_required: j
                .get("attendance_required")
                .and_then(Json::as_str)
                .map(str::to_string),
            occurrence_key_fn: j
                .get("occurrence_key_fn")
                .and_then(Json::as_str)
                .map(str::to_string),
        })
    }
}

/// One occurrence's fold state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OccurrenceState {
    /// `occurred` recorded, not yet claimed/fired.
    Pending,
    /// `fired` recorded (the claim won).
    Fired,
    /// `skipped` recorded (the audit reason rides the row).
    Skipped,
}

/// The fold's view of one occurrence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    /// The `occurrence_key`.
    pub key: String,
    /// The `control.wakeup.occurred` event id.
    pub occurred_event_id: String,
    /// Its seq.
    pub occurred_seq: u64,
    /// The payload ref, when carried.
    pub payload_ref: Option<String>,
    /// The state.
    pub state: OccurrenceState,
    /// `deliver_after` — the committed effect this delivery waits on (W-3).
    pub deliver_after: Option<String>,
    /// The `control.wakeup.fired` event, when fired.
    pub fired_event_id: Option<String>,
    /// The skip reason, when skipped.
    pub skipped_reason: Option<String>,
}

/// `SubscriptionState ∈ {active, fired, cancelled, expired}` — folded, never
/// stored (the rows are the record).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionState {
    /// Live — occurrences may fire.
    Active,
    /// Fired at least once and has no pending occurrences (one-shot spent).
    Fired,
    /// `control.wakeup.cancelled` landed.
    Cancelled,
    /// `policy.expires_at` passed.
    Expired,
}

/// `WakeupSubscription` — the fold over `control.wakeup.*`.
#[derive(Debug, Clone, PartialEq)]
pub struct WakeupSubscription {
    /// The subscription id.
    pub subscription_id: String,
    /// The owner (`run_id`; goal-owners are Stage 4).
    pub owner: String,
    /// The trigger.
    pub trigger: Trigger,
    /// The policy.
    pub policy: WakeupPolicy,
    /// The subscribing event's coordinate.
    pub created_by: EventRef,
    /// The folded state.
    pub state: SubscriptionState,
    /// `occurrence_key → fold` in arrival order.
    pub occurrences: BTreeMap<String, Occurrence>,
}

/// What `wakeup_fire` / `deliver_wakeup` resolved for one occurrence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FireOutcome {
    /// `fired` landed (`deliver_after` names the blocking committed effect when
    /// W-3 applies).
    Fired {
        /// The `control.wakeup.fired` event.
        event_id: String,
        /// The committed effect this delivery waits on, if any.
        deliver_after: Option<String>,
    },
    /// `skipped{reason}` landed.
    Skipped {
        /// The audited reason.
        reason: String,
        /// The `control.wakeup.skipped` event.
        event_id: String,
    },
}

/// A deliverable wakeup — `wakeup_drain`'s element (`Cue.woken`'s data).
#[derive(Debug, Clone, PartialEq)]
pub struct WokenDelivery {
    /// The subscription.
    pub subscription_id: String,
    /// The occurrence.
    pub occurrence_key: String,
    /// The trigger.
    pub trigger: Trigger,
    /// The payload ref, when carried.
    pub payload_ref: Option<String>,
    /// The delivery mode (`follow_up` at this stage).
    pub delivery_mode: DeliveryMode,
}

impl Store {
    /// `subscribe(owner, trigger, policy, created_by)` — the durable
    /// `control.wakeup.scheduled` row; the subscription id is allocated.
    /// Refusals: non-Stage-2 trigger ⇒ `TriggerUnsupported`; `steer` ⇒
    /// `WakeupPolicyUnsupported`; over [`MAX_WAKEUP_SUBSCRIPTIONS`] ⇒
    /// `SubscriptionLimit`.
    pub fn wakeup_subscribe(
        &mut self,
        run_id: &str,
        lease: &Lease,
        trigger: Trigger,
        policy: WakeupPolicy,
        created_by: &EventRef,
    ) -> Result<String, LedgerError> {
        self.tier_c1("wakeup_subscribe")?;
        if !trigger.admissible() {
            return Err(LedgerError::TriggerUnsupported {
                trigger: trigger.type_name().to_string(),
                stage: 4,
            });
        }
        if policy.delivery_mode == DeliveryMode::Steer {
            return Err(LedgerError::WakeupPolicyUnsupported {
                detail: "delivery_mode = steer — OQ-316 ratifies follow_up only \
                         at this stage"
                    .into(),
            });
        }
        let count = self
            .run(run_id)?
            .wakeups
            .values()
            .filter(|s| s.state == SubscriptionState::Active)
            .count();
        if count >= MAX_WAKEUP_SUBSCRIPTIONS {
            return Err(LedgerError::SubscriptionLimit {
                run_id: run_id.to_string(),
                count,
            });
        }
        // A `child_terminal` subscription names a run this store must hold —
        // an unresolvable child could never fire (CC3 nothing unpinned).
        if let Trigger::ChildTerminal { child_run_id } = &trigger {
            self.run(child_run_id)?;
        }
        let subscription_id = self.alloc_id("wsub");
        let ev = kernel_ev(
            self,
            run_id,
            "control.wakeup.scheduled",
            Scope::default(),
            Json::obj([
                (
                    "subscription",
                    Json::obj([
                        ("subscription_id", Json::str(&subscription_id)),
                        ("owner", Json::str(run_id)),
                        ("trigger", trigger.to_json()),
                        ("policy", policy.to_json()),
                    ]),
                ),
                ("created_by", crate::leases::event_ref_json(created_by)),
            ]),
        )?;
        self.append(run_id, lease, vec![ev])?;
        Ok(subscription_id)
    }

    /// `record_occurrence(subscription, occurrence_key, payload_ref,
    /// observed_at)` — the ingress half (§5g `respond` → occurrence; the
    /// kernel-internal triggers synthesize theirs inside [`Store::deliver_wakeup`]).
    /// `occurred` is durable before any `fired`; a duplicate
    /// `(subscription, occurrence_key)` is `skipped{duplicate_occurrence}` —
    /// audited, never a double-fire.
    pub fn wakeup_occurred(
        &mut self,
        run_id: &str,
        lease: &Lease,
        subscription_id: &str,
        occurrence_key: &str,
        payload_ref: Option<&str>,
        observed_at_ms: u64,
    ) -> Result<OccurOutcome, LedgerError> {
        self.tier_c1("wakeup_occurred")?;
        let sub_exists = self.run(run_id)?.wakeups.contains_key(subscription_id);
        if !sub_exists {
            return Err(LedgerError::UnknownSubscription {
                subscription_id: subscription_id.to_string(),
            });
        }
        if self
            .run(run_id)?
            .wakeups
            .get(subscription_id)
            .and_then(|s| s.occurrences.get(occurrence_key))
            .is_some()
        {
            // The duplicate occurrence is an audited skip, never a second fire.
            let ev = kernel_ev(
                self,
                run_id,
                "control.wakeup.skipped",
                Scope::default(),
                Json::obj([
                    ("subscription_id", Json::str(subscription_id)),
                    ("occurrence_key", Json::str(occurrence_key)),
                    ("reason", Json::str("duplicate_occurrence")),
                ]),
            )?;
            return self
                .append(run_id, lease, vec![ev])
                .map(OccurOutcome::Skipped);
        }
        let mut payload = BTreeMap::from([
            ("subscription_id".to_string(), Json::str(subscription_id)),
            ("occurrence_key".to_string(), Json::str(occurrence_key)),
            ("observed_at".to_string(), Json::Int(observed_at_ms as i64)),
        ]);
        if let Some(r) = payload_ref {
            payload.insert("payload_ref".to_string(), Json::str(r));
        }
        let ev = kernel_ev(
            self,
            run_id,
            "control.wakeup.occurred",
            Scope::default(),
            Json::Obj(payload),
        )?;
        self.append(run_id, lease, vec![ev])
            .map(OccurOutcome::Occurred)
    }

    /// `fire(subscription, occurrence, holder)` — the claim: one atomic batch
    /// of `lifecycle.lease.acquired{scope: wakeup(sub,key)}` +
    /// `control.wakeup.fired`. Every refusal path is an audited `skipped`.
    pub fn wakeup_fire(
        &mut self,
        run_id: &str,
        lease: &Lease,
        subscription_id: &str,
        occurrence_key: &str,
    ) -> Result<FireOutcome, LedgerError> {
        self.tier_c1("wakeup_fire")?;
        self.fire_one(run_id, lease, subscription_id, occurrence_key)
    }

    /// The occurrence-synthesis + claim engine — the kernel-internal trigger
    /// pass. Scans every active subscription, materialises newly-true
    /// occurrences (`control.wakeup.occurred` — durable first), then fires
    /// each under the W-1 claim discipline. Idempotent: a `deliver_wakeup`
    /// after a crash rediscovers `occurred`-not-`fired` rows (KP-11).
    pub fn deliver_wakeup(
        &mut self,
        run_id: &str,
        lease: &Lease,
        now_ms: u64,
    ) -> Result<Vec<FireOutcome>, LedgerError> {
        self.tier_c1("deliver_wakeup")?;
        // ── synthesize occurrences (durable before any fire) ──────────
        let due = self.wakeup_due(run_id, now_ms)?;
        for (sub_id, key, _payload_ref) in &due {
            let already = self
                .run(run_id)?
                .wakeups
                .get(sub_id)
                .and_then(|s| s.occurrences.get(key))
                .is_some();
            if !already {
                self.wakeup_occurred(run_id, lease, sub_id, key, None, now_ms)?;
            }
        }
        // ── fire every pending occurrence ──────────────────────────────
        let pending: Vec<(String, String)> = self
            .run(run_id)?
            .wakeups
            .iter()
            .flat_map(|(id, s)| {
                s.occurrences
                    .values()
                    .filter(|o| o.state == OccurrenceState::Pending)
                    .map(|o| (id.clone(), o.key.clone()))
                    .collect::<Vec<_>>()
            })
            .collect();
        let mut out = Vec::new();
        for (sub_id, key) in pending {
            out.push(self.fire_one(run_id, lease, &sub_id, &key)?);
        }
        Ok(out)
    }

    /// `due(now) → [(subscription_id, occurrence_key, payload_ref)]` — the pure
    /// projection of which kernel-internal occurrences would materialise at
    /// `now` (spec's `due(now)`; restart-safe — same ledger, same answer).
    pub fn wakeup_due(
        &self,
        run_id: &str,
        now_ms: u64,
    ) -> Result<Vec<(String, String, Option<String>)>, LedgerError> {
        let mut out = Vec::new();
        let subs: Vec<WakeupSubscription> = self.run(run_id)?.wakeups.values().cloned().collect();
        for sub in subs {
            if !matches!(
                sub.state,
                SubscriptionState::Active | SubscriptionState::Fired
            ) {
                continue;
            }
            if let Some(exp) = sub.policy.expires_at_ms {
                if now_ms >= exp {
                    continue;
                }
            }
            match &sub.trigger {
                Trigger::Timer { at_ms } => {
                    if *at_ms <= now_ms {
                        out.push((sub.subscription_id.clone(), format!("timer:{at_ms}"), None));
                    }
                }
                Trigger::PermissionDecided { permission_id } => {
                    let decided = self.events(run_id)?.iter().any(|e| {
                        e.class == "security.permission.decided"
                            && e.payload.get("permission_id").and_then(Json::as_str)
                                == Some(permission_id.as_str())
                    });
                    if decided {
                        out.push((
                            sub.subscription_id.clone(),
                            format!("permission:{permission_id}"),
                            None,
                        ));
                    }
                }
                Trigger::ChildTerminal { child_run_id } => {
                    if self.run(child_run_id).map(|c| c.finished).unwrap_or(false) {
                        out.push((
                            sub.subscription_id.clone(),
                            format!("child:{child_run_id}"),
                            None,
                        ));
                    }
                }
                Trigger::EffectTerminal { effect_id } => {
                    let done = self
                        .run(run_id)?
                        .effects
                        .get(effect_id)
                        .is_some_and(|f| f.is_terminal());
                    if done {
                        out.push((
                            sub.subscription_id.clone(),
                            format!("effect:{effect_id}"),
                            None,
                        ));
                    }
                }
                Trigger::RetryDue { scope_id } => {
                    for t in self.retry_due(run_id, now_ms)? {
                        if t.scope_id == *scope_id {
                            let key = match t.attempt_no {
                                Some(a) => format!("retry:{scope_id}:{a}"),
                                None => format!("retry:{scope_id}"),
                            };
                            out.push((sub.subscription_id.clone(), key, None));
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(out)
    }

    /// `wakeup_drain(run)` — the deliverable set: `fired` occurrences whose
    /// `deliver_after` effect is absent or terminal (W-3 — a `follow_up`
    /// delivery never lands mid-effect). Pure projection; the caller emits
    /// `Cue.woken` per element (the `(subscription, occurrence)` pair is the
    /// delivery dedup key).
    pub fn wakeup_drain(&self, run_id: &str) -> Result<Vec<WokenDelivery>, LedgerError> {
        let st = self.run(run_id)?;
        let mut out = Vec::new();
        for s in st.wakeups.values() {
            for o in s.occurrences.values() {
                if o.state != OccurrenceState::Fired {
                    continue;
                }
                let blocked = o
                    .deliver_after
                    .as_ref()
                    .is_some_and(|eid| st.effects.get(eid).is_some_and(|f| !f.is_terminal()));
                if !blocked {
                    out.push(WokenDelivery {
                        subscription_id: s.subscription_id.clone(),
                        occurrence_key: o.key.clone(),
                        trigger: s.trigger.clone(),
                        payload_ref: o.payload_ref.clone(),
                        delivery_mode: s.policy.delivery_mode,
                    });
                }
            }
        }
        Ok(out)
    }

    /// `cancel_subscription(subscription, reason)` — `control.wakeup.
    /// cancelled`; pending occurrences are `skipped{subscription_cancelled}`
    /// in the same batch (the audited skip, never silent).
    pub fn wakeup_cancel(
        &mut self,
        run_id: &str,
        lease: &Lease,
        subscription_id: &str,
        reason: &str,
    ) -> Result<(), LedgerError> {
        self.tier_c1("wakeup_cancel")?;
        let pending: Vec<String> = self
            .run(run_id)?
            .wakeups
            .get(subscription_id)
            .ok_or_else(|| LedgerError::UnknownSubscription {
                subscription_id: subscription_id.to_string(),
            })?
            .occurrences
            .values()
            .filter(|o| o.state == OccurrenceState::Pending)
            .map(|o| o.key.clone())
            .collect();
        let mut batch = Vec::new();
        for key in pending {
            batch.push(kernel_ev(
                self,
                run_id,
                "control.wakeup.skipped",
                Scope::default(),
                Json::obj([
                    ("subscription_id", Json::str(subscription_id)),
                    ("occurrence_key", Json::str(&key)),
                    ("reason", Json::str("subscription_cancelled")),
                ]),
            )?);
        }
        batch.push(kernel_ev(
            self,
            run_id,
            "control.wakeup.cancelled",
            Scope::default(),
            Json::obj([
                ("subscription_id", Json::str(subscription_id)),
                ("reason", Json::str(reason)),
            ]),
        )?);
        self.append(run_id, lease, batch)?;
        Ok(())
    }

    /// The subscription fold (a read view — the rows are the record).
    pub fn wakeup_subscriptions(
        &self,
        run_id: &str,
    ) -> Result<Vec<WakeupSubscription>, LedgerError> {
        Ok(self.run(run_id)?.wakeups.values().cloned().collect())
    }

    /// The claim discipline — shared by `wakeup_fire` and `deliver_wakeup`.
    /// Appends the audited `skipped` row on every non-fire path.
    fn fire_one(
        &mut self,
        run_id: &str,
        lease: &Lease,
        subscription_id: &str,
        occurrence_key: &str,
    ) -> Result<FireOutcome, LedgerError> {
        let st = self.run(run_id)?;
        let Some(sub) = st.wakeups.get(subscription_id).cloned() else {
            return Err(LedgerError::UnknownSubscription {
                subscription_id: subscription_id.to_string(),
            });
        };
        let Some(occ) = sub.occurrences.get(occurrence_key).cloned() else {
            return Err(LedgerError::UnknownOccurrence {
                subscription_id: subscription_id.to_string(),
                occurrence_key: occurrence_key.to_string(),
            });
        };
        let now = self.now_ms();
        // The audited non-fire paths (one machine — every branch appends the
        // `skipped` row it names).
        macro_rules! skip {
            ($reason:expr) => {{
                let ev = kernel_ev(
                    self,
                    run_id,
                    "control.wakeup.skipped",
                    Scope::default(),
                    Json::obj([
                        ("subscription_id", Json::str(subscription_id)),
                        ("occurrence_key", Json::str(occurrence_key)),
                        ("reason", Json::str($reason)),
                    ]),
                )?;
                let event_id = ev.event_id.clone();
                self.append(run_id, lease, vec![ev])?;
                return Ok(FireOutcome::Skipped {
                    reason: $reason.to_string(),
                    event_id,
                });
            }};
        }
        if st.finished {
            // The run's log is sealed — no post-terminal row can land
            // (`append` refuses `RunFinished`), so the skip is the returned
            // verdict: `finished` is itself the durable record, and the
            // outcome names the audit that would have been written.
            return Ok(FireOutcome::Skipped {
                reason: "run_finished".to_string(),
                event_id: occ.occurred_event_id.clone(),
            });
        }
        if sub.state == SubscriptionState::Cancelled {
            skip!("subscription_cancelled");
        }
        if let Some(exp) = sub.policy.expires_at_ms {
            if now >= exp {
                skip!("expired");
            }
        }
        match occ.state {
            OccurrenceState::Fired => skip!("already_claimed"),
            OccurrenceState::Skipped => {
                // Idempotent re-fire of an audited skip — a no-op read, not a
                // second row (the first skip is the record).
                return Ok(FireOutcome::Skipped {
                    reason: occ.skipped_reason.clone().unwrap_or_default(),
                    event_id: occ.occurred_event_id.clone(),
                });
            }
            OccurrenceState::Pending => {}
        }
        // `max_pending` — the live (pending ∪ fired-undelivered) bound.
        let live = sub
            .occurrences
            .values()
            .filter(|o| o.state != OccurrenceState::Skipped)
            .count() as u64;
        if live > sub.policy.max_pending {
            skip!("over_max_pending");
        }
        // W-3: `follow_up` while an effect is `committed` — the fire lands but
        // the delivery waits on the effect's terminal.
        let deliver_after = if sub.policy.delivery_mode == DeliveryMode::FollowUp {
            self.run(run_id)?
                .effects
                .values()
                .find(|f| f.phase == crate::effect::EffectPhase::Committed)
                .map(|f| f.effect_id.clone())
        } else {
            None
        };
        // W-5: `coalesce = latest|all` — older pending occurrences are
        // `skipped{coalesced}`; `all` additionally coalesces a fired-but-
        // blocked occurrence into this one.
        let mut batch = Vec::new();
        if sub.policy.coalesce != Coalesce::None {
            for o in sub.occurrences.values() {
                let older_pending = o.state == OccurrenceState::Pending && o.key != occ.key;
                let older_blocked = sub.policy.coalesce == Coalesce::All
                    && o.state == OccurrenceState::Fired
                    && o.deliver_after.is_some()
                    && o.key != occ.key;
                if older_pending || older_blocked {
                    batch.push(kernel_ev(
                        self,
                        run_id,
                        "control.wakeup.skipped",
                        Scope::default(),
                        Json::obj([
                            ("subscription_id", Json::str(subscription_id)),
                            ("occurrence_key", Json::str(&o.key)),
                            ("reason", Json::str("coalesced")),
                        ]),
                    )?);
                }
            }
        }
        // The claim: `lifecycle.lease.acquired{scope: wakeup(sub,key)}` +
        // `control.wakeup.fired` in ONE batch — atomic (no claim-without-fire
        // torn state; a crash before the batch commits nothing).
        let claim = kernel_ev(
            self,
            run_id,
            "lifecycle.lease.acquired",
            Scope::default(),
            Json::obj([
                (
                    "scope",
                    Json::str(
                        LeaseScope::Wakeup {
                            subscription_id: subscription_id.to_string(),
                            occurrence_key: occurrence_key.to_string(),
                        }
                        .spelling(),
                    ),
                ),
                ("lease_id", Json::str(self.alloc_id("lease"))),
                ("holder", Json::str(&lease.holder)),
                ("generation", Json::Int(lease.generation as i64)),
            ]),
        )?;
        batch.push(claim);
        let mut fired_payload = BTreeMap::from([
            ("subscription_id".to_string(), Json::str(subscription_id)),
            ("occurrence_key".to_string(), Json::str(occurrence_key)),
            (
                "occurred_event".to_string(),
                Json::str(&occ.occurred_event_id),
            ),
            ("holder".to_string(), Json::str(&lease.holder)),
        ]);
        if let Some(eid) = &deliver_after {
            fired_payload.insert("deliver_after".to_string(), Json::str(eid));
        }
        if let Some(pr) = &occ.payload_ref {
            fired_payload.insert("payload_ref".to_string(), Json::str(pr));
        }
        let fired = kernel_ev(
            self,
            run_id,
            "control.wakeup.fired",
            Scope::default(),
            Json::Obj(fired_payload),
        )?;
        let fired_id = fired.event_id.clone();
        batch.push(fired);
        self.append(run_id, lease, batch)?;
        Ok(FireOutcome::Fired {
            event_id: fired_id,
            deliver_after,
        })
    }
}

/// The two outcomes of [`Store::wakeup_occurred`] (a duplicate is an audited
/// skip, not an error).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OccurOutcome {
    /// The `control.wakeup.occurred` range.
    Occurred(crate::event::SeqRange),
    /// The `control.wakeup.skipped{duplicate_occurrence}` range.
    Skipped(crate::event::SeqRange),
}

/// The wakeup fold arm — `control.wakeup.*` rows → `RunState.wakeups`
/// (rebuild and commit share this; CC1).
pub(crate) fn fold_wakeup_row(
    wakeups: &mut BTreeMap<String, WakeupSubscription>,
    env: &crate::event::EventEnvelope,
) {
    let p = &env.payload;
    let sid = |p: &Json| {
        p.get("subscription_id")
            .and_then(Json::as_str)
            .map(str::to_string)
    };
    match env.class.as_str() {
        "control.wakeup.scheduled" => {
            let Some(sub) = p.get("subscription") else {
                return;
            };
            let (Some(id), Some(trigger), Some(policy)) = (
                sid(sub),
                Trigger::from_json(sub.get("trigger").unwrap_or(&Json::Null)),
                WakeupPolicy::from_json(sub.get("policy").unwrap_or(&Json::obj([]))),
            ) else {
                return;
            };
            let created_by = sub_event_ref(p.get("created_by"), env);
            wakeups.insert(
                id.clone(),
                WakeupSubscription {
                    subscription_id: id,
                    owner: sub
                        .get("owner")
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    trigger,
                    policy,
                    created_by,
                    state: SubscriptionState::Active,
                    occurrences: BTreeMap::new(),
                },
            );
        }
        "control.wakeup.occurred" => {
            let (Some(id), Some(key)) = (sid(p), str_at(p, "occurrence_key")) else {
                return;
            };
            if let Some(s) = wakeups.get_mut(&id) {
                s.occurrences.entry(key.clone()).or_insert(Occurrence {
                    key,
                    occurred_event_id: env.event_id.clone(),
                    occurred_seq: env.seq,
                    payload_ref: str_at(p, "payload_ref"),
                    state: OccurrenceState::Pending,
                    deliver_after: None,
                    fired_event_id: None,
                    skipped_reason: None,
                });
            }
        }
        "control.wakeup.fired" => {
            let (Some(id), Some(key)) = (sid(p), str_at(p, "occurrence_key")) else {
                return;
            };
            if let Some(s) = wakeups.get_mut(&id) {
                if let Some(o) = s.occurrences.get_mut(&key) {
                    o.state = OccurrenceState::Fired;
                    o.fired_event_id = Some(env.event_id.clone());
                    o.deliver_after = str_at(p, "deliver_after");
                }
            }
        }
        "control.wakeup.skipped" => {
            let (Some(id), Some(key)) = (sid(p), str_at(p, "occurrence_key")) else {
                return;
            };
            // A `duplicate_occurrence` skip is about the *re-delivered*
            // occurrence — it never entered the table, so the live pending
            // occurrence under the same key stays Pending (the skip row is
            // the audit, not a state transition on the original).
            if str_at(p, "reason").as_deref() == Some("duplicate_occurrence") {
                return;
            }
            if let Some(s) = wakeups.get_mut(&id) {
                if let Some(o) = s.occurrences.get_mut(&key) {
                    if o.state == OccurrenceState::Pending {
                        o.state = OccurrenceState::Skipped;
                        o.skipped_reason = str_at(p, "reason");
                    }
                }
            }
        }
        "control.wakeup.cancelled" => {
            if let Some(id) = sid(p) {
                if let Some(s) = wakeups.get_mut(&id) {
                    s.state = SubscriptionState::Cancelled;
                }
            }
        }
        _ => {}
    }
}

fn str_at(p: &Json, k: &str) -> Option<String> {
    p.get(k).and_then(Json::as_str).map(str::to_string)
}

fn sub_event_ref(j: Option<&Json>, env: &crate::event::EventEnvelope) -> EventRef {
    j.and_then(|r| {
        Some(EventRef {
            run_id: str_at(r, "run_id")?,
            event_id: str_at(r, "event_id")?,
        })
    })
    .unwrap_or(EventRef {
        run_id: env.run_id.clone(),
        event_id: env.event_id.clone(),
    })
}
