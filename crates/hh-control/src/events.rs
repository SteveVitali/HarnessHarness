//! The `control.*` ledger-row payload builders (§5e.1/§5e.2 ledger rows;
//! ADR-0106/0107/0108). Every builder returns the canonical payload `Json` —
//! the driver wraps it in an [`hh_ledger::event::Event`] with a kernel
//! [`Producer`]; the class shapes are the registered `classes.rs` rows
//! (`control.decision`, `control.retry.scheduled`, `control.timeout.fired`,
//! `control.loop.detected`, `control.output.rejected`,
//! `control.invariant.violated`, `control.budget.exceeded`,
//! `lifecycle.run.finished`). No free text — ids, enums, ints and refs only
//! (the refusal `detail_ref` is a content address, never rejected bytes).

use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::manifest::EventRef;
use hh_ontology::control::{InvariantId, StopReason};
use hh_wire::json::Json;

use crate::policy::LadderAction;
use crate::state::PlanCursor;
use crate::state::{decision_point_str, owner_str};
use crate::vocab::{ControlDecision, Decider, GuardPoint, ScopeKind, SettledOutcome};

/// `HH-CONTROL` — the producer component id for every envelope/driver row.
pub const COMPONENT: &str = "hh-control";

/// The kernel producer stamp for this crate's rows.
pub fn producer() -> Producer {
    Producer::kernel(COMPONENT)
}

/// `control.decision` (§5e.1; the S1.20 full payload `{decision_id,
/// decision_point, owner, checkpoint_ref, cursor, verdict, decider,
/// triggered_by, reason?, kind, submission_ref?}`).
#[allow(clippy::too_many_arguments)] // the payload row's arity is the record's
pub fn decision_payload(
    decision_id: &str,
    d: &ControlDecision,
    cursor: &PlanCursor,
    checkpoint_ref: &str,
    decider: Decider,
    triggered_by: &[EventRef],
    verdict: &Json,
    admitted_reason: Option<&StopReason>,
) -> Json {
    let mut m = vec![
        ("decision_id", Json::str(decision_id)),
        ("kind", Json::str(d.kind.as_str())),
        (
            "decision_point",
            Json::str(decision_point_str(d.stamp.decision_point)),
        ),
        ("owner", Json::str(owner_str(d.stamp.owner))),
        ("cursor", cursor.to_json()),
        ("checkpoint_ref", Json::str(checkpoint_ref)),
        ("decider", Json::str(decider.as_str())),
        (
            "triggered_by",
            Json::Arr(
                triggered_by
                    .iter()
                    .map(|r| Json::str(&r.event_id))
                    .collect(),
            ),
        ),
        ("verdict", verdict.clone()),
    ];
    if let Some(r) = admitted_reason {
        m.push(("reason", r.to_json()));
    }
    if let crate::vocab::DecisionKind::Stop {
        submission_ref: Some(s),
        ..
    } = &d.kind
    {
        m.push(("submission_ref", Json::str(s)));
    }
    if let Some(r) = &d.stamp.rationale_ref {
        m.push(("rationale_ref", Json::str(r)));
    }
    Json::obj(m)
}

/// `control.retry.scheduled{scope_kind, scope_id, attempt_no, error_class,
/// delay_ms, policy_ref, attempt_delta?}` (§5e.2; the durable timer — process
/// memory is never the only copy, ADR-0130 §5).
#[allow(clippy::too_many_arguments)]
pub fn retry_scheduled_payload(
    kind: ScopeKind,
    scope_id: &str,
    attempt_no: u64,
    error_class: &str,
    delay_ms: u64,
    not_before: u64,
    policy_ref: &str,
    attempt_delta: Option<&crate::vocab::DeltaKind>,
) -> Json {
    let mut m = vec![
        ("scope_kind", Json::str(kind.as_str())),
        ("scope_id", Json::str(scope_id)),
        ("attempt_no", Json::Int(attempt_no as i64)),
        ("error_class", Json::str(error_class)),
        ("delay_ms", Json::Int(delay_ms as i64)),
        ("not_before", Json::Int(not_before as i64)),
        ("policy_ref", Json::str(policy_ref)),
    ];
    if let Some(d) = attempt_delta {
        m.push(("attempt_delta", Json::str(d.as_str())));
    }
    Json::obj(m)
}

/// `control.timeout.fired{scope_kind, scope_id, deadline, hard_max,
/// terminal_event, action}` (§5e.2).
pub fn timeout_fired_payload(
    kind: ScopeKind,
    scope_id: &str,
    deadline: u64,
    hard_max: u64,
    terminal_event: &str,
    action: &str,
) -> Json {
    Json::obj([
        ("scope_kind", Json::str(kind.as_str())),
        ("scope_id", Json::str(scope_id)),
        ("deadline", Json::Int(deadline as i64)),
        ("hard_max", Json::Int(hard_max as i64)),
        ("terminal_event", Json::str(terminal_event)),
        ("action", Json::str(action)),
    ])
}

/// `control.loop.detected{detector, pattern{cycle_len, repeats, loop_keys[]},
/// evidence_refs[], action, ladder_position, validator_ref?}` (§5e.2).
pub fn loop_detected_payload(
    detector: hh_ontology::control::LoopDetectorKind,
    pattern: &hh_ontology::control::LoopPattern,
    evidence_refs: &[String],
    action: LadderAction,
    ladder_position: u32,
    validator_ref: Option<&str>,
) -> Json {
    let mut m = vec![
        ("detector", Json::str(detector.as_str())),
        (
            "pattern",
            Json::obj([
                ("cycle_len", Json::Int(pattern.cycle_len as i64)),
                ("repeats", Json::Int(pattern.repeats as i64)),
                (
                    "loop_keys",
                    Json::Arr(pattern.loop_keys.iter().map(Json::str).collect()),
                ),
            ]),
        ),
        (
            "evidence_refs",
            Json::Arr(evidence_refs.iter().map(Json::str).collect()),
        ),
        ("action", Json::str(action.as_str())),
        ("ladder_position", Json::Int(ladder_position as i64)),
    ];
    if let Some(v) = validator_ref {
        m.push(("validator_ref", Json::str(v)));
    }
    Json::obj(m)
}

/// `control.output.rejected{model_call_id, failure_class, surface_id?,
/// detail_ref, repaired, format_failures_running}` — `detail_ref` is the
/// redaction-safe content address; **no rejected bytes ride the row**
/// (§5e.2).
pub fn output_rejected_payload(
    model_call_id: &str,
    failure_class: &str,
    surface_id: Option<&str>,
    detail_ref: &str,
    repaired: bool,
    format_failures_running: u32,
) -> Json {
    let mut m = vec![
        ("model_call_id", Json::str(model_call_id)),
        ("failure_class", Json::str(failure_class)),
        ("detail_ref", Json::str(detail_ref)),
        ("repaired", Json::Bool(repaired)),
        (
            "format_failures_running",
            Json::Int(format_failures_running as i64),
        ),
    ];
    if let Some(s) = surface_id {
        m.push(("surface_id", Json::str(s)));
    }
    Json::obj(m)
}

/// `control.invariant.violated{invariant_id, evidence_refs[],
/// detected_at_guard}` (§5e.2 — precedes `stop{invariant_violation}` and the
/// quarantine checkpoint).
pub fn invariant_violated_payload(
    invariant_id: &InvariantId,
    evidence_refs: &[String],
    detected_at_guard: GuardPoint,
) -> Json {
    Json::obj([
        ("invariant_id", Json::str(invariant_id.as_str())),
        (
            "evidence_refs",
            Json::Arr(evidence_refs.iter().map(Json::str).collect()),
        ),
        ("detected_at_guard", Json::str(detected_at_guard.as_str())),
    ])
}

/// `control.budget.exceeded{budget_id, dimension, value, limit, authority}`
/// (the audit row — the envelope records an exhaustion crossing, never an
/// `amend` that loosens, INV-7).
pub fn budget_exceeded_payload(budget_id: &str, dimension: &str, value: i64, limit: i64) -> Json {
    Json::obj([
        ("budget_id", Json::str(budget_id)),
        ("dimension", Json::str(dimension)),
        ("value", Json::Int(value)),
        ("limit", Json::Int(limit)),
        ("authority", Json::str("envelope")),
    ])
}

/// `lifecycle.run.finished{status, stop_reason, outcome_class,
/// unresolved_effects[], drain_report_ref}` (§5e.2 stop-protocol step 7).
pub fn run_finished_payload(
    status: &str,
    reason: &StopReason,
    unresolved_effects: &[String],
    drain_report_ref: &str,
) -> Json {
    Json::obj([
        ("status", Json::str(status)),
        ("stop_reason", reason.to_json()),
        ("outcome_class", Json::str(reason.outcome_class().as_str())),
        (
            "unresolved_effects",
            Json::Arr(unresolved_effects.iter().map(Json::str).collect()),
        ),
        ("drain_report_ref", Json::str(drain_report_ref)),
    ])
}

/// `lifecycle.turn.finished{stop_reason}` — the turn close.
pub fn turn_finished_payload(reason: &StopReason) -> Json {
    Json::obj([("stop_reason", reason.to_json())])
}

/// `control.guard.fired{decision_point, guard_id}` — the ledgered spelling of
/// the `guard_fired` cue (CF-229).
pub fn guard_fired_payload(
    decision_point: hh_ontology::control::DecisionPoint,
    guard_id: &str,
) -> Json {
    Json::obj([
        (
            "decision_point",
            Json::str(decision_point_str(decision_point)),
        ),
        ("guard_id", Json::str(guard_id)),
    ])
}

/// `security.audit.checkpoint{kind: quarantine}` — the INV-violation
/// quarantine record (`invariant_id` + evidence ride the checkpoint payload).
pub fn quarantine_payload(invariant_id: &InvariantId, evidence_refs: &[String]) -> Json {
    Json::obj([
        ("kind", Json::str("quarantine")),
        ("invariant_id", Json::str(invariant_id.as_str())),
        (
            "evidence_refs",
            Json::Arr(evidence_refs.iter().map(Json::str).collect()),
        ),
    ])
}

/// Build a kernel-produced [`Event`] in `scope` (the caller allocates
/// `event_id`, `ts`, `parent_event_id`, `causes` — the store stamps the
/// rest).
pub fn kernel_event(
    event_id: String,
    class: &str,
    ts: String,
    scope: Scope,
    parent_event_id: String,
    causes: Vec<EventRef>,
    payload: Json,
) -> Event {
    Event {
        event_id,
        class: class.into(),
        ts,
        hlc: None,
        producer: producer(),
        scope,
        parent_event_id,
        causes,
        refs: vec![],
        ir_refs: vec![],
        surface_ids: std::collections::BTreeMap::new(),
        provenance: None,
        content_kind: None,
        payload,
    }
}

/// The settled-outcome member spelling for a `control.decision` verdict or a
/// drain report entry.
pub fn settled_outcome_json(o: &SettledOutcome) -> Json {
    match o {
        SettledOutcome::Observed { outcome } => Json::obj([
            ("terminal", Json::str("observed")),
            ("outcome", Json::str(outcome)),
        ]),
        SettledOutcome::Refused => Json::obj([("terminal", Json::str("refused"))]),
        SettledOutcome::Unknown { cause } => Json::obj([
            ("terminal", Json::str("unknown")),
            ("cause", Json::str(cause)),
        ]),
        SettledOutcome::Abandoned => Json::obj([("terminal", Json::str("abandoned"))]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::{DecisionKind, DecisionStamp};
    use hh_ontology::control::DecisionPoint;
    use hh_ontology::control::Owner;

    #[test]
    fn decision_payload_carries_the_full_stamp() {
        let d = ControlDecision {
            stamp: DecisionStamp {
                decision_point: DecisionPoint::Act,
                owner: Owner::Model,
                rationale_ref: None,
            },
            kind: DecisionKind::Wait {
                until: crate::vocab::WaitUntil::CueKind {
                    cue_kind: "effects_settled".into(),
                },
            },
        };
        let j = decision_payload(
            "d-1",
            &d,
            &PlanCursor {
                node_id: "n".into(),
                iteration: 0,
                bound_ref: "b".into(),
            },
            "ckpt-1",
            Decider::Strategy,
            &[],
            &Json::str("admitted"),
            None,
        );
        assert_eq!(j.get("decision_point").and_then(Json::as_str), Some("act"));
        assert_eq!(j.get("owner").and_then(Json::as_str), Some("model"));
        assert_eq!(j.get("decider").and_then(Json::as_str), Some("strategy"));
        assert_eq!(
            j.get("checkpoint_ref").and_then(Json::as_str),
            Some("ckpt-1")
        );
    }

    #[test]
    fn output_rejected_carries_no_bytes() {
        let j = output_rejected_payload("m-1", "unparseable", None, "sha256:abc", false, 2);
        assert!(j.get("bytes").is_none());
        assert_eq!(
            j.get("detail_ref").and_then(Json::as_str),
            Some("sha256:abc")
        );
    }
}
