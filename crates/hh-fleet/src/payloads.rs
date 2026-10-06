//! The `control.work_item.*` / `lifecycle.fleet.activated` /
//! `lifecycle.escalation.*` / `context.observation.recorded` payload
//! builders + decoders — the Rule-C contract. Every member here is named
//! in `hh-ledger`'s declared partition for the class (an unknown member is
//! a schema error at append, so the builders and the partition table move
//! together — CC7).
//!
//! `control.work_item.dispatched` is the item's lifecycle record class —
//! `verb` discriminates the row:
//! - `admit` — the full admission dossier (state = `queued`);
//! - `dispatch` — RC-2's lease+mark (`dispatch{spec_ref, lease_ref,
//!   acquire, started_ts, status:dispatching, declared?}`);
//! - `run`/`error` — `fleet.dispatch_note`'s stamps;
//! - `retry` — RC-5's durable retry decision (`retry{attempts,
//!   next_attempt_at_ms, spec_ref}`);
//! - `source_conflict` — RC-3's conflict record.

use hh_ledger::event::EventEnvelope;
use hh_wire::json::Json;
use std::collections::BTreeMap;

use crate::errors::FleetError;
use crate::identity::{js, PAYLOAD_SCHEMA, SPEC_SCHEMA};
use crate::spec::FleetSpec;
use crate::work_item::{ItemOn, WorkItemInit};

// ── lifecycle.fleet.activated ────────────────────────────────────────────

/// The activation row — the FleetSpec's durable record (the manifest's
/// `extra.fleet_spec` carries the same canonical object; the row is the
/// audit-visible copy with `spec_ref`).
pub fn activated_payload(spec: &FleetSpec, spec_ref: &str) -> Json {
    Json::obj([
        ("schema", js(SPEC_SCHEMA)),
        ("spec_ref", js(spec_ref)),
        ("spec", spec.to_json()),
        ("fixture_ref", js(&spec.fixture_ref)),
        ("policy_ref", js(&spec.policy_ref)),
        (
            "budget_ref",
            spec.budget_ref.as_ref().map(js).unwrap_or(Json::Null),
        ),
        ("purpose", js(&spec.purpose)),
    ])
}

/// Decode a `lifecycle.fleet.activated` payload → `(spec, spec_ref)`.
pub fn spec_from_activated(payload: &Json) -> Result<(FleetSpec, String), FleetError> {
    let spec =
        FleetSpec::from_json(
            payload
                .get("spec")
                .ok_or_else(|| FleetError::InvalidPayload {
                    detail: "lifecycle.fleet.activated missing spec".into(),
                })?,
        )?;
    let spec_ref = payload
        .get("spec_ref")
        .and_then(Json::as_str)
        .ok_or_else(|| FleetError::InvalidPayload {
            detail: "lifecycle.fleet.activated missing spec_ref".into(),
        })?
        .to_string();
    Ok((spec, spec_ref))
}

// ── context.observation.recorded ─────────────────────────────────────────

/// The durable source-observation row — `{trigger{type,kind?,occurrence_id},
/// item, observed_at_ms, adapter, source_ref}` (§5i.1 #2's observe row).
pub fn observation_payload(
    trigger_kind: &str,
    external_kind: Option<&str>,
    occurrence_id: &str,
    item: &WorkItemInit,
    observed_at_ms: u64,
    adapter: &str,
    source_ref: &str,
) -> Json {
    let mut trig = BTreeMap::new();
    trig.insert("type".into(), js(trigger_kind));
    if let Some(k) = external_kind {
        trig.insert("kind".into(), js(k));
    }
    trig.insert("occurrence_id".into(), js(occurrence_id));
    Json::obj([
        ("schema", js(PAYLOAD_SCHEMA)),
        ("trigger", Json::Obj(trig)),
        ("item", item_init_json(item)),
        ("observed_at_ms", Json::Int(observed_at_ms as i64)),
        ("adapter", js(adapter)),
        ("source_ref", js(source_ref)),
    ])
}

/// `WorkItemInit` → the `item` member (dossier verbatim).
pub fn item_init_json(init: &WorkItemInit) -> Json {
    let mut m = BTreeMap::new();
    m.insert("item_id".into(), js(&init.item_id));
    m.insert("title".into(), js(&init.title));
    m.insert("source".into(), init.source.clone());
    if let Some(o) = &init.occurrence {
        m.insert("occurrence".into(), o.clone());
    }
    m.insert("idempotency_key".into(), js(&init.idempotency_key));
    if let Some(owner) = &init.owner {
        m.insert("owner".into(), js(owner));
    }
    if !init.blocking.is_empty() {
        m.insert(
            "blocking".into(),
            Json::Arr(init.blocking.iter().map(js).collect()),
        );
    }
    m.insert("on".into(), init.on.to_json());
    Json::Obj(m)
}

// ── control.work_item.dispatched ─────────────────────────────────────────

/// The `admit` row — the full dossier at `state = queued`.
pub fn admit_payload(run_id: &str, init: &WorkItemInit, spec_ref: &str) -> Json {
    let run_item_id = crate::identity::run_item_id(run_id, &init.item_id);
    Json::obj([
        ("schema", js(PAYLOAD_SCHEMA)),
        ("verb", js("admit")),
        ("item_id", js(&init.item_id)),
        ("run_item_id", js(&run_item_id)),
        ("title", js(&init.title)),
        ("source", init.source.clone()),
        ("occurrence", init.occurrence.clone().unwrap_or(Json::Null)),
        ("idempotency_key", js(&init.idempotency_key)),
        (
            "work_item_ref",
            js(crate::identity::work_item_ref(run_id, &init.item_id)),
        ),
        ("owner", init.owner.as_ref().map(js).unwrap_or(Json::Null)),
        (
            "blocking",
            Json::Arr(init.blocking.iter().map(js).collect()),
        ),
        ("spec_ref", js(spec_ref)),
        ("on", init.on.to_json()),
    ])
}

/// The `dispatch` row — RC-2's lease+mark.
pub fn dispatch_mark_payload(
    item_id: &str,
    run_item_id: &str,
    spec_ref: &str,
    lease_ref: &str,
    acquire: &str,
    started_ts: u64,
    declared: Option<Json>,
) -> Json {
    let mut d = BTreeMap::new();
    d.insert("spec_ref".into(), js(spec_ref));
    d.insert("lease_ref".into(), js(lease_ref));
    d.insert("acquire".into(), js(acquire));
    d.insert("started_ts".into(), Json::Int(started_ts as i64));
    d.insert("status".into(), js("dispatching"));
    if let Some(decl) = declared {
        d.insert("declared".into(), decl);
    }
    Json::obj([
        ("schema", js(PAYLOAD_SCHEMA)),
        ("verb", js("dispatch")),
        ("item_id", js(item_id)),
        ("run_item_id", js(run_item_id)),
        ("spec_ref", js(spec_ref)),
        ("dispatch", Json::Obj(d)),
    ])
}

/// The `run` row — `dispatch_note`'s `run_ref` stamp.
pub fn run_note_payload(item_id: &str, run_item_id: &str, spec_ref: &str, run_ref: &str) -> Json {
    Json::obj([
        ("schema", js(PAYLOAD_SCHEMA)),
        ("verb", js("run")),
        ("item_id", js(item_id)),
        ("run_item_id", js(run_item_id)),
        ("spec_ref", js(spec_ref)),
        (
            "dispatch",
            Json::obj([
                ("spec_ref", js(spec_ref)),
                ("run_ref", js(run_ref)),
                ("status", js("dispatched")),
            ]),
        ),
    ])
}

/// The `error` row — `dispatch_note`'s `last_error` stamp.
pub fn error_note_payload(
    item_id: &str,
    run_item_id: &str,
    spec_ref: &str,
    last_error: &str,
) -> Json {
    Json::obj([
        ("schema", js(PAYLOAD_SCHEMA)),
        ("verb", js("error")),
        ("item_id", js(item_id)),
        ("run_item_id", js(run_item_id)),
        ("spec_ref", js(spec_ref)),
        (
            "dispatch",
            Json::obj([
                ("spec_ref", js(spec_ref)),
                ("last_error", js(last_error)),
                ("status", js("error")),
            ]),
        ),
    ])
}

/// The `retry` row — RC-5's durable retry decision.
pub fn retry_payload(
    item_id: &str,
    run_item_id: &str,
    spec_ref: &str,
    attempts: u64,
    next_attempt_at_ms: u64,
) -> Json {
    Json::obj([
        ("schema", js(PAYLOAD_SCHEMA)),
        ("verb", js("retry")),
        ("item_id", js(item_id)),
        ("run_item_id", js(run_item_id)),
        ("spec_ref", js(spec_ref)),
        (
            "retry",
            Json::obj([
                ("spec_ref", js(spec_ref)),
                ("attempts", Json::Int(attempts as i64)),
                ("next_attempt_at_ms", Json::Int(next_attempt_at_ms as i64)),
            ]),
        ),
    ])
}

/// The `source_conflict` row — RC-3's conflict record (`field` names the
/// divergent member).
pub fn source_conflict_payload(
    item_id: &str,
    run_item_id: &str,
    field: &str,
    expected: &str,
    seen: &str,
) -> Json {
    Json::obj([
        ("schema", js(PAYLOAD_SCHEMA)),
        ("verb", js("source_conflict")),
        ("item_id", js(item_id)),
        ("run_item_id", js(run_item_id)),
        ("field", js(field)),
        ("expected", js(expected)),
        ("seen", js(seen)),
    ])
}

/// The `source_update` row — a source `state` transition on an admitted
/// item (canonical reconcile step (2); the durable record the human-gate
/// fold reads).
pub fn source_update_payload(item_id: &str, run_item_id: &str, state: &str) -> Json {
    Json::obj([
        ("schema", js(PAYLOAD_SCHEMA)),
        ("verb", js("source_update")),
        ("item_id", js(item_id)),
        ("run_item_id", js(run_item_id)),
        ("source", Json::obj([("state", js(state))])),
    ])
}

/// `dispatched{verb:"bind_source"}` — the `WorkSourceBinding` record
/// (`{role ∈ origin|mirror|subscription, source_id, native_id?}` — a PR or
/// thread the agent opens lands as a `subscription` binding on the SAME
/// item, never a new one — ADR-0205 D5).
pub fn bind_source_payload(item_id: &str, run_item_id: &str, binding: &Json) -> Json {
    Json::obj([
        ("schema", js(PAYLOAD_SCHEMA)),
        ("verb", js("bind_source")),
        ("item_id", js(item_id)),
        ("run_item_id", js(run_item_id)),
        ("binding", binding.clone()),
    ])
}

/// `dispatched{verb:"resume"}` — `resume_from_handoff`'s human-act record
/// (§5i.1 #2's exit rule: "only by a human act observed from the source or
/// through a surface").
pub fn resume_payload(item_id: &str, run_item_id: &str, resumed_by: &str) -> Json {
    Json::obj([
        ("schema", js(PAYLOAD_SCHEMA)),
        ("verb", js("resume")),
        ("item_id", js(item_id)),
        ("run_item_id", js(run_item_id)),
        ("resumed_by", js(resumed_by)),
    ])
}

// ── control.work_item.blocked ────────────────────────────────────────────

/// `blocked{verb:add}` — one live cause lands (`code` is the closed cause
/// spelling; `escalation_ref`/`issue_ref` join the escalation pair when the
/// cause IS the raise).
pub fn block_add_payload(
    item_id: &str,
    run_item_id: &str,
    code: &str,
    issue_ref: Option<&str>,
    escalation_ref: Option<&str>,
) -> Json {
    Json::obj([
        ("schema", js(PAYLOAD_SCHEMA)),
        ("verb", js("add")),
        ("item_id", js(item_id)),
        ("run_item_id", js(run_item_id)),
        ("code", js(code)),
        ("issue_ref", issue_ref.map(js).unwrap_or(Json::Null)),
        (
            "escalation_ref",
            escalation_ref.map(js).unwrap_or(Json::Null),
        ),
    ])
}

/// `blocked{verb:remove}` — RC-5/RC-7's re-check clears one cause.
pub fn block_remove_payload(item_id: &str, run_item_id: &str, code: &str) -> Json {
    Json::obj([
        ("schema", js(PAYLOAD_SCHEMA)),
        ("verb", js("remove")),
        ("item_id", js(item_id)),
        ("run_item_id", js(run_item_id)),
        ("code", js(code)),
    ])
}

// ── control.work_item.{stopped,cancelled,handoff,owner_changed,owner_acknowledged} ──

/// `stopped` — `verb ∈ {settle, stop, terminal}`; `settle` carries the
/// `Outcome` + `evidence_refs` (AC-7), `stop`/`terminal` carry `reason`.
pub fn stopped_payload(
    verb: &str,
    item_id: &str,
    run_item_id: &str,
    outcome: Option<&str>,
    reason: Option<&str>,
    evidence_refs: &[String],
) -> Json {
    Json::obj([
        ("schema", js(PAYLOAD_SCHEMA)),
        ("verb", js(verb)),
        ("item_id", js(item_id)),
        ("run_item_id", js(run_item_id)),
        ("outcome", outcome.map(js).unwrap_or(Json::Null)),
        ("reason", reason.map(js).unwrap_or(Json::Null)),
        (
            "evidence_refs",
            Json::Arr(evidence_refs.iter().map(js).collect()),
        ),
    ])
}

/// `cancelled{reason, cancelled_by}` — the explicit cancel row.
pub fn cancelled_payload(item_id: &str, run_item_id: &str, reason: &str, by: &str) -> Json {
    Json::obj([
        ("schema", js(PAYLOAD_SCHEMA)),
        ("item_id", js(item_id)),
        ("run_item_id", js(run_item_id)),
        ("reason", js(reason)),
        ("cancelled_by", js(by)),
    ])
}

/// `handoff{item, handoff_ref, from_agent, to_agent, lease_agent_ref, basis}` —
/// the kernel-produced state-handoff record (the escalation handoff carries
/// the `lease_agent` surrogate — §5i.1; the item enters state `handoff` and
/// the durable row moves ownership to `to_agent`).
pub fn handoff_payload(
    item_id: &str,
    run_item_id: &str,
    handoff_ref: &str,
    from_agent: &str,
    to_agent: &str,
    lease_agent_ref: &str,
    basis: &str,
) -> Json {
    Json::obj([
        ("schema", js(PAYLOAD_SCHEMA)),
        ("item_id", js(item_id)),
        ("run_item_id", js(run_item_id)),
        ("handoff_ref", js(handoff_ref)),
        ("from_agent", js(from_agent)),
        ("to_agent", js(to_agent)),
        ("lease_agent_ref", js(lease_agent_ref)),
        ("basis", js(basis)),
    ])
}

/// `owner_changed{item, agent, owner}` — a graph edge landed (the `item_id`
/// is the calling context; `agent`/`owner` are the edge's vertices).
pub fn owner_changed_payload(item_id: &str, run_item_id: &str, agent: &str, owner: &str) -> Json {
    Json::obj([
        ("schema", js(PAYLOAD_SCHEMA)),
        ("item_id", js(item_id)),
        ("run_item_id", js(run_item_id)),
        ("agent", js(agent)),
        ("owner", js(owner)),
    ])
}

/// `owner_changed{item, from, to, basis}` — the item-ownership transfer
/// record (canonical O-4's first event; `owner_acknowledged{by = to}` is
/// the second). The `{from, to, basis}` spelling is the transfer; the
/// `{agent, owner}` spelling is the fleet graph edge — the fold keys on
/// member presence.
pub fn transfer_owner_payload(
    item_id: &str,
    run_item_id: &str,
    from: &str,
    to: &str,
    basis: &str,
) -> Json {
    Json::obj([
        ("schema", js(PAYLOAD_SCHEMA)),
        ("item_id", js(item_id)),
        ("run_item_id", js(run_item_id)),
        ("from", js(from)),
        ("to", js(to)),
        ("basis", js(basis)),
    ])
}

/// `owner_acknowledged{item, agent, lease_agent_ref, ack}` — the durable
/// acknowledgement (`ack` is a boolean — a revocation lands `ack:false`).
pub fn owner_acknowledged_payload(
    item_id: &str,
    run_item_id: &str,
    agent: &str,
    lease_agent_ref: &str,
) -> Json {
    Json::obj([
        ("schema", js(PAYLOAD_SCHEMA)),
        ("item_id", js(item_id)),
        ("run_item_id", js(run_item_id)),
        ("agent", js(agent)),
        ("lease_agent_ref", js(lease_agent_ref)),
        ("ack", Json::Bool(true)),
    ])
}

// ── lifecycle.escalation.{raised,resolved} ───────────────────────────────

/// `lifecycle.escalation.raised` — the durable raise (a `timer` wakeup
/// arms `deadline_ms` when present; `deadline_sub` names the subscription
/// id so the fold can map fired rows back).
#[allow(clippy::too_many_arguments)] // the raised row carries the full member set
pub fn escalation_raised_payload(
    item_id: &str,
    run_item_id: &str,
    issue: &str,
    issue_ref: &str,
    escalation_ref: &str,
    raised_by: &str,
    raised_no: u64,
    cause: &str,
    deadline_ms: Option<u64>,
    deadline_sub: Option<&str>,
    handoff: Option<(&str, &str)>, // (to_agent, lease_agent_ref)
) -> Json {
    let mut m = BTreeMap::new();
    m.insert("schema".into(), js(PAYLOAD_SCHEMA));
    m.insert("item_id".into(), js(item_id));
    m.insert("run_item_id".into(), js(run_item_id));
    m.insert("issue".into(), js(issue));
    m.insert("issue_ref".into(), js(issue_ref));
    m.insert("escalation_ref".into(), js(escalation_ref));
    m.insert("raised_by".into(), js(raised_by));
    m.insert("raised_no".into(), Json::Int(raised_no as i64));
    m.insert("cause".into(), js(cause));
    if let Some(d) = deadline_ms {
        m.insert("deadline_ms".into(), Json::Int(d as i64));
    }
    if let Some(s) = deadline_sub {
        m.insert("deadline_sub".into(), js(s));
    }
    if let Some((to, lease)) = handoff {
        m.insert(
            "handoff".into(),
            Json::obj([("to_agent", js(to)), ("lease_agent_ref", js(lease))]),
        );
    }
    Json::Obj(m)
}

/// `lifecycle.escalation.resolved` — the durable resolution; the fold
/// clears the item's open escalation and drops the `escalation` block
/// cause (`unblocked` records whether the row cleared it).
#[allow(clippy::too_many_arguments)] // the resolved row carries the full member set
pub fn escalation_resolved_payload(
    item_id: &str,
    run_item_id: &str,
    issue_ref: &str,
    escalation_ref: &str,
    resolved_by: &str,
    resolution_kind: &str,
    note: Option<&str>,
    resolution_count: u64,
) -> Json {
    let mut m = BTreeMap::new();
    m.insert("schema".into(), js(PAYLOAD_SCHEMA));
    m.insert("item_id".into(), js(item_id));
    m.insert("run_item_id".into(), js(run_item_id));
    m.insert("issue_ref".into(), js(issue_ref));
    m.insert("escalation_ref".into(), js(escalation_ref));
    m.insert("resolved_by".into(), js(resolved_by));
    m.insert(
        "resolution".into(),
        Json::Obj(BTreeMap::from([
            ("kind".into(), js(resolution_kind)),
            ("note".into(), note.map(js).unwrap_or(Json::Null)),
        ])),
    );
    m.insert(
        "resolution_count".into(),
        Json::Int(resolution_count as i64),
    );
    Json::Obj(m)
}

/// `control.work_item.annotated{item, subject, subject_ref?, text_ref,
/// readers[], annotated_by}` — the fleet-run annotation row (plane 5;
/// ADR-0205 D7 — never delivered to a model except through a `HarnessRule`
/// with `context.artefact.delivered` events).
pub fn annotated_payload(
    item_id: &str,
    run_item_id: &str,
    subject: &str,
    subject_ref: Option<&Json>,
    text_ref: &str,
    readers: &[String],
    annotated_by: &str,
) -> Json {
    Json::obj([
        ("schema", js(PAYLOAD_SCHEMA)),
        ("item_id", js(item_id)),
        ("run_item_id", js(run_item_id)),
        ("subject", js(subject)),
        ("subject_ref", subject_ref.cloned().unwrap_or(Json::Null)),
        ("text_ref", js(text_ref)),
        ("readers", Json::Arr(readers.iter().map(js).collect())),
        ("annotated_by", js(annotated_by)),
    ])
}

// ── decoders (the fold's `WorkItemPatch` direction) ──────────────────────

/// `verb` of a `control.work_item.*` payload (missing ⇒ the class's own
/// default — `dispatched` rows without `verb` are `admit`-shaped only at
/// parse time; every fleet row carries it explicitly).
pub fn verb(payload: &Json) -> &str {
    payload
        .get("verb")
        .and_then(Json::as_str)
        .unwrap_or("admit")
}

/// The `item_id` of a work-item payload — every row names it.
pub fn item_id_of(payload: &Json) -> Option<&str> {
    payload.get("item_id").and_then(Json::as_str)
}

/// The `run_item_id` of a work-item payload.
pub fn run_item_id_of(payload: &Json) -> Option<&str> {
    payload.get("run_item_id").and_then(Json::as_str)
}

/// The admission dossier members a `verb:admit` row carries → `WorkItemInit`
/// (fold-side decode — tolerant: a durable row the fleet wrote round-trips
/// exactly; `occurrence: null` decodes to `None`).
pub fn init_from_admit(payload: &Json) -> Result<WorkItemInit, FleetError> {
    let s = |k: &str| -> Result<String, FleetError> {
        payload
            .get(k)
            .and_then(Json::as_str)
            .map(str::to_string)
            .ok_or_else(|| FleetError::InvalidPayload {
                detail: format!("admit row missing {k}"),
            })
    };
    Ok(WorkItemInit {
        item_id: s("item_id")?,
        title: s("title")?,
        source: payload.get("source").cloned().unwrap_or(Json::Null),
        occurrence: match payload.get("occurrence") {
            Some(Json::Null) | None => None,
            v => v.cloned(),
        },
        idempotency_key: s("idempotency_key")?,
        owner: payload
            .get("owner")
            .and_then(Json::as_str)
            .map(str::to_string),
        blocking: match payload.get("blocking") {
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect(),
            _ => Vec::new(),
        },
        on: match payload.get("on") {
            Some(v) => ItemOn::from_json(v)?,
            None => ItemOn::default(),
        },
    })
}

/// The `dispatch` member of a `verb:dispatch|run|error` row.
pub fn dispatch_of(payload: &Json) -> Option<&Json> {
    payload.get("dispatch")
}

/// The `retry` member of a `verb:retry` row.
pub fn retry_of(payload: &Json) -> Option<&Json> {
    payload.get("retry")
}

/// Extract the fleet-relevant members of an envelope for the fold — the
/// caller already knows `env.class`; `(item_id, verb)` resolves the row's
/// subject.
pub fn subject_of(env: &EventEnvelope) -> (Option<String>, String) {
    (
        item_id_of(&env.payload).map(str::to_string),
        verb(&env.payload).to_string(),
    )
}
