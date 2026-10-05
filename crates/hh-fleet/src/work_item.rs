//! `WorkItem` — the admission input (`WorkItemInit`, what `admit` and the
//! fixture occurrences carry) and `WorkItemView` (the fold's synthesized
//! state — §5i.1 #3's field set verbatim).

use hh_embed_schema::types::NarrowingLeaf;
use hh_wire::json::Json;
use std::collections::{BTreeMap, BTreeSet};

use crate::errors::FleetError;
use crate::identity::js;

/// `on` — the item's durable behaviour switches (§5i.1 #3). Every member is
/// a closed record; unknown members fail closed at the codec.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemOn {
    /// `stuck{after_ms}` — RC-5's stall detector.
    pub stuck_after_ms: Option<u64>,
    /// `blocked{escalate: {to, deadline_ms?}?}` — the blocked-path
    /// escalation declaration.
    pub blocked_escalate: Option<BlockedEscalate>,
    /// `dispatched{escalate}` — the post-dispatch escalation declaration
    /// (same shape as `blocked_escalate`).
    pub dispatched_escalate: Option<BlockedEscalate>,
    /// `dispatch_error{retry: {max_attempts, backoff_ms}}` — RC-5's retry arm.
    pub retry_max_attempts: Option<u64>,
    /// `on.retry.backoff_ms`.
    pub retry_backoff_ms: Option<u64>,
    /// `stall{escalate}` — the stalled-item escalation declaration.
    pub stall_escalate: Option<BlockedEscalate>,
    /// `ack_required` — the item's dispatch class requires owner
    /// acknowledgement before it may dispatch (§5i.1 #4: "`ack` false only
    /// when the work item's dispatch class requires acknowledgement" — the
    /// declaration is durable on the item, never a host convention).
    pub ack_required: bool,
    /// The item's own Π narrowing leaves (state-map per-row members).
    pub narrowing: Vec<NarrowingLeaf>,
}

/// `blocked{escalate:{to, deadline_ms?}}` — one escalation declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockedEscalate {
    /// The escalation target agent.
    pub to: String,
    /// Optional deadline — a `timer` wakeup re-raises `overdue` at expiry.
    pub deadline_ms: Option<u64>,
}

/// `WorkItemInit` — the admission record a `fleet.admit` or fixture
/// occurrence presents. The durable `control.work_item.dispatched` row
/// carries this dossier verbatim (plus the synthesized `run_item_id` /
/// `spec_ref` / `work_item_ref`).
#[derive(Debug, Clone, PartialEq)]
pub struct WorkItemInit {
    /// `item_id` — caller/fleet-scoped; `run_item_id` derives from it.
    pub item_id: String,
    /// The title (a short label — never content).
    pub title: String,
    /// The work source coordinate (`source_id`/`kind`/`source_ref`).
    pub source: Json,
    /// The producing occurrence `{trigger, occurrence_id}` when admitted
    /// from a source observation.
    pub occurrence: Option<Json>,
    /// `idempotency_key` — dedup within the activation.
    pub idempotency_key: String,
    /// `owner` — an `agents[]` member or `None` (owner-less until
    /// `set_owner`; `none` + `dispatch to network` ⇒ `owner_required`).
    pub owner: Option<String>,
    /// `blocking` — child item ids this item blocks (RC-7; cycles ⇒
    /// `cycle` at admit).
    pub blocking: Vec<String>,
    /// The behaviour switches.
    pub on: ItemOn,
}

/// `dispatch{…}` — the folded dispatch block (§5i.1 #3).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DispatchState {
    /// The spec the dispatch decision ran under (RC-1's stale leg).
    pub spec_ref: Option<String>,
    /// The dispatched run — `fleet.dispatch_note`'s stamp.
    pub run_ref: Option<String>,
    /// The scoped dispatch lease — RC-2's preconditions.
    pub lease_ref: Option<String>,
    /// The `dispatching` timestamp (the fire's `now`).
    pub started_ts: Option<u64>,
    /// `ready|dispatching|dispatched|error`.
    pub status: String,
    /// `last_error` — the recorded host dispatch error.
    pub last_error: Option<String>,
    /// `declared` — the matched-budget slice the dispatch carried (RC-6).
    pub declared: Option<Json>,
    /// The scoped-lease acquire mode recorded at fire (`acquire|upgrade`).
    pub acquire: Option<String>,
}

/// `retry{…}` — the folded retry state (§5i.1 #3).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RetryState {
    /// The spec the last retry decision ran under.
    pub spec_ref: Option<String>,
    /// `next_attempt_at_ms` — the durable retry timer.
    pub next_attempt_at_ms: Option<u64>,
    /// `attempts` — dispatch errors consumed so far.
    pub attempts: u64,
    /// `escalation_count` — the escalation bound counter.
    pub escalation_count: u64,
}

/// `escalation{…}` — the folded open escalation (§5i.1 #3).
#[derive(Debug, Clone, PartialEq)]
pub struct EscalationState {
    /// `issue_ref` — `H(item ∥ issue)`.
    pub issue_ref: String,
    /// `escalation_ref` — `H(item ∥ issue ∥ raised_no)`.
    pub escalation_ref: String,
    /// The issue code (`blocked|stalled|source_suspended|budget|conflict|…`).
    pub issue: String,
    /// The raising agent (or `local_admin`).
    pub raised_by: String,
    /// `deadline_ms` when declared.
    pub deadline_ms: Option<u64>,
    /// `status = open`.
    pub status: String,
    /// `raised_no` — this item's Nth escalation (part of `escalation_ref`).
    pub raised_no: u64,
    /// `handoff` — the `lease_agent` the escalation carried, when it
    /// transferred ownership.
    pub handoff: Option<Json>,
    /// `cause` — the durable member (`blocked`/`stale_lease`/`manual`/…).
    pub cause: String,
    /// The `timer` subscription id the deadline armed, when armed.
    pub deadline_sub: Option<String>,
}

/// `work_item.settlement` — the settle record the fold exposes.
#[derive(Debug, Clone, PartialEq)]
pub struct Settlement {
    /// The `Outcome` (`completed|failed|cancelled|abandoned`).
    pub outcome: String,
    /// `evidence_refs[]` — provenance refs the settle names.
    pub evidence_refs: Vec<String>,
}

/// `WorkItemView` — the fold's per-item record (§5i.1 #3's full set:
/// `run_item_id`, `item_id`, `title`, `source`, `occurrence`,
/// `idempotency_key`, `work_item_ref`, `owner`, `blocking`, `spec_ref`,
/// `on`, `dispatch`, `retry`, `escalation`, `settlement`, `blocked`,
/// `suspended`, `state`, `dead`, plus `watch{state} ∈ {watching,dead}`
/// for the host).
#[derive(Debug, Clone, PartialEq)]
pub struct WorkItemView {
    /// `H(fleet_run ∥ item_id)` — the fleet-scoped coordinate.
    pub run_item_id: String,
    /// The admission's `item_id`.
    pub item_id: String,
    /// The title.
    pub title: String,
    /// The work source coordinate.
    pub source: Json,
    /// The producing occurrence record.
    pub occurrence: Option<Json>,
    /// `idempotency_key`.
    pub idempotency_key: String,
    /// `work_item_ref` (= `run_item_id` — canonicalized once).
    pub work_item_ref: String,
    /// `owner` — `Option<agent_ref>` plus the folded `ack`/`lease_agent_ref`.
    pub owner: Option<String>,
    /// `owner_acknowledged` folded state.
    pub owner_ack: bool,
    /// The `lease_agent` in force (`lease_agent_ref` from ack/handoff).
    pub lease_agent_ref: Option<String>,
    /// `blocking` — child item ids this item blocks.
    pub blocking: Vec<String>,
    /// `spec_ref` — the FleetSpec this item was admitted under.
    pub spec_ref: String,
    /// `on`.
    pub on: ItemOn,
    /// `dispatch{…}`.
    pub dispatch: DispatchState,
    /// `retry{…}`.
    pub retry: RetryState,
    /// `escalation{…}` — the open escalation, when present.
    pub escalation: Option<EscalationState>,
    /// `work_item.settlement` — present once `settle` lands.
    pub settlement: Option<Settlement>,
    /// `blocked` — the set of live block causes (RC-5/RC-7 codes).
    pub blocked: BTreeSet<String>,
    /// `suspended` — the source suspension state.
    pub suspended: bool,
    /// `state` — `queued|dispatching|dispatched|blocked|handoff|terminal`.
    pub state: String,
    /// `watch{state}` — `watching` while live, `dead` after `stop`/settle.
    pub watch_state: String,
    /// The admission `control.work_item.dispatched` event id — the durable
    /// coordinate `causes`/`issue_ref`s name.
    pub admitted_event_id: String,
    /// The most recent `verb:"dispatch"` row's event id — the child run's
    /// `spawn_event` anchor (the fleet_anchor obligation).
    pub dispatch_event_id: Option<String>,
}

/// The closed `state` spellings (§5i.1 #4's fold + terminal).
pub const STATES: &[&str] = &[
    "queued",
    "dispatching",
    "dispatched",
    "blocked",
    "handoff",
    "terminal",
];

/// `queued → dispatching → dispatched → blocked → handoff → terminal`
/// precedence — the fold recomputes `state` from the durable member set
/// (never a stored field — the fold derives it, restore-safe).
pub fn derive_state(v: &WorkItemView) -> String {
    if v.watch_state == "dead" || v.settlement.is_some() {
        return "terminal".into();
    }
    if !v.blocked.is_empty() || v.escalation.is_some() {
        // `human_gate` alone is the handoff *state* (§5i.1 #4's
        // `blocked → handoff → terminal` ordering; an item carrying any
        // other live cause — or an open escalation — reports `blocked`).
        if v.blocked.iter().all(|c| c == "human_gate") && v.escalation.is_none() {
            return "handoff".into();
        }
        return "blocked".into();
    }
    match v.dispatch.status.as_str() {
        "dispatching" => "dispatching".into(),
        "dispatched" => "dispatched".into(),
        // `error` without a durable block cause still surfaces
        // `blocked` (the retry path arms its own cause through
        // `verb:retry`; a non-retryable error is operator-blocked).
        "error" => "blocked".into(),
        _ => "queued".into(),
    }
}

/// Whether a state transition is admissible under §5i.1 #4's ordering —
/// `queued → dispatching → dispatched → blocked → handoff → terminal`,
/// `blocked` is re-entrant; `handoff` returns to `dispatched`/`queued`.
pub fn transition_ok(from: &str, to: &str) -> bool {
    match (from, to) {
        ("queued", "dispatching" | "blocked" | "terminal") => true,
        ("dispatching", "dispatched" | "blocked" | "terminal") => true,
        ("dispatched", "blocked" | "handoff" | "terminal") => true,
        ("blocked", "dispatched" | "queued" | "handoff" | "terminal") => true,
        ("handoff", "dispatched" | "queued" | "terminal") => true,
        _ => false,
    }
}

impl ItemOn {
    /// The canonical `on` member.
    pub fn to_json(&self) -> Json {
        let mut m = BTreeMap::new();
        if let Some(t) = self.stuck_after_ms {
            m.insert(
                "stuck".into(),
                Json::obj([("after_ms", Json::Int(t as i64))]),
            );
        }
        if let Some(e) = &self.blocked_escalate {
            let mut esc = BTreeMap::new();
            esc.insert("to".into(), js(&e.to));
            if let Some(d) = e.deadline_ms {
                esc.insert("deadline_ms".into(), Json::Int(d as i64));
            }
            m.insert("blocked".into(), Json::obj([("escalate", Json::Obj(esc))]));
        }
        if let Some(e) = &self.dispatched_escalate {
            let mut esc = BTreeMap::new();
            esc.insert("to".into(), js(&e.to));
            if let Some(d) = e.deadline_ms {
                esc.insert("deadline_ms".into(), Json::Int(d as i64));
            }
            m.insert(
                "dispatched".into(),
                Json::obj([("escalate", Json::Obj(esc))]),
            );
        }
        if self.retry_max_attempts.is_some() || self.retry_backoff_ms.is_some() {
            let mut r = BTreeMap::new();
            if let Some(n) = self.retry_max_attempts {
                r.insert("max_attempts".into(), Json::Int(n as i64));
            }
            if let Some(b) = self.retry_backoff_ms {
                r.insert("backoff_ms".into(), Json::Int(b as i64));
            }
            m.insert("retry".into(), Json::Obj(r));
        }
        if let Some(e) = &self.stall_escalate {
            let mut esc = BTreeMap::new();
            esc.insert("to".into(), js(&e.to));
            if let Some(d) = e.deadline_ms {
                esc.insert("deadline_ms".into(), Json::Int(d as i64));
            }
            m.insert("stall".into(), Json::obj([("escalate", Json::Obj(esc))]));
        }
        if self.ack_required {
            m.insert("ack_required".into(), Json::Bool(true));
        }
        if !self.narrowing.is_empty() {
            m.insert(
                "narrowing".into(),
                Json::Arr(self.narrowing.iter().map(|l| l.to_json()).collect()),
            );
        }
        Json::Obj(m)
    }

    /// Strict decode — unknown members fail closed.
    pub fn from_json(j: &Json) -> Result<ItemOn, FleetError> {
        let Json::Obj(o) = j else {
            return Err(FleetError::SchemaViolation {
                detail: "item on must be an object".into(),
            });
        };
        const KNOWN: &[&str] = &[
            "stuck",
            "blocked",
            "dispatched",
            "dispatch_error",
            "retry",
            "stall",
            "ack_required",
            "narrowing",
        ];
        for k in o.keys() {
            if !KNOWN.contains(&k.as_str()) {
                return Err(FleetError::SchemaViolation {
                    detail: format!("item on unknown member {k}"),
                });
            }
        }
        let esc_of = |v: &Json, path: &str| -> Result<Option<BlockedEscalate>, FleetError> {
            match v.get("escalate") {
                None | Some(Json::Null) => Ok(None),
                Some(e) => {
                    let to = e
                        .get("to")
                        .and_then(Json::as_str)
                        .ok_or_else(|| FleetError::SchemaViolation {
                            detail: format!("{path}.escalate.to required"),
                        })?
                        .to_string();
                    let deadline_ms = e
                        .get("deadline_ms")
                        .and_then(Json::as_int)
                        .map(|i| i.max(0) as u64);
                    Ok(Some(BlockedEscalate { to, deadline_ms }))
                }
            }
        };
        let retry = o
            .get("retry")
            .or_else(|| o.get("dispatch_error").and_then(|d| d.get("retry")));
        let (max_attempts, backoff) = match retry {
            None | Some(Json::Null) => (None, None),
            Some(r) => (
                r.get("max_attempts")
                    .and_then(Json::as_int)
                    .map(|i| i.max(0) as u64),
                r.get("backoff_ms")
                    .and_then(Json::as_int)
                    .map(|i| i.max(0) as u64),
            ),
        };
        let narrowing = match o.get("narrowing") {
            None | Some(Json::Null) => Vec::new(),
            Some(Json::Arr(a)) => {
                let mut out = Vec::new();
                for v in a {
                    out.push(NarrowingLeaf::from_json(v, "/on/narrowing").map_err(|e| {
                        FleetError::SchemaViolation {
                            detail: format!("item narrowing leaf: {e:?}"),
                        }
                    })?);
                }
                out
            }
            Some(_) => {
                return Err(FleetError::SchemaViolation {
                    detail: "item on.narrowing must be an array".into(),
                })
            }
        };
        Ok(ItemOn {
            stuck_after_ms: o
                .get("stuck")
                .and_then(|s| s.get("after_ms"))
                .and_then(Json::as_int)
                .map(|i| i.max(0) as u64),
            blocked_escalate: esc_of(o.get("blocked").unwrap_or(&Json::Null), "/on/blocked")?,
            dispatched_escalate: esc_of(
                o.get("dispatched").unwrap_or(&Json::Null),
                "/on/dispatched",
            )?,
            retry_max_attempts: max_attempts,
            retry_backoff_ms: backoff,
            stall_escalate: esc_of(o.get("stall").unwrap_or(&Json::Null), "/on/stall")?,
            ack_required: matches!(o.get("ack_required"), Some(Json::Bool(true))),
            narrowing,
        })
    }
}

impl WorkItemInit {
    /// The strict wire decode (`fleet.admit`'s `item` param and the fixture
    /// occurrence's `item` member share the shape).
    pub fn from_json(j: &Json) -> Result<WorkItemInit, FleetError> {
        let Json::Obj(o) = j else {
            return Err(FleetError::SchemaViolation {
                detail: "work item must be an object".into(),
            });
        };
        const KNOWN: &[&str] = &[
            "item_id",
            "title",
            "source",
            "occurrence",
            "idempotency_key",
            "owner",
            "blocking",
            "on",
        ];
        for k in o.keys() {
            if !KNOWN.contains(&k.as_str()) {
                return Err(FleetError::SchemaViolation {
                    detail: format!("work item unknown member {k}"),
                });
            }
        }
        let s = |k: &str| -> Result<String, FleetError> {
            o.get(k)
                .and_then(Json::as_str)
                .map(str::to_string)
                .ok_or_else(|| FleetError::SchemaViolation {
                    detail: format!("work item {k} required"),
                })
        };
        Ok(WorkItemInit {
            item_id: s("item_id")?,
            title: s("title")?,
            source: o
                .get("source")
                .cloned()
                .ok_or_else(|| FleetError::SchemaViolation {
                    detail: "work item source required".into(),
                })?,
            occurrence: o.get("occurrence").cloned(),
            idempotency_key: crate::identity::idempotency_key(
                &s("item_id")?,
                o.get("idempotency_key").and_then(Json::as_str),
            ),
            owner: o.get("owner").and_then(Json::as_str).map(str::to_string),
            blocking: match o.get("blocking") {
                None | Some(Json::Null) => Vec::new(),
                Some(Json::Arr(a)) => a
                    .iter()
                    .filter_map(Json::as_str)
                    .map(str::to_string)
                    .collect(),
                Some(_) => {
                    return Err(FleetError::SchemaViolation {
                        detail: "work item blocking must be an array".into(),
                    })
                }
            },
            on: match o.get("on") {
                None | Some(Json::Null) => ItemOn::default(),
                Some(v) => ItemOn::from_json(v)?,
            },
        })
    }
}
