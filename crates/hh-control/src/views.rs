//! The materialized envelope views (§5e.2 `loop_state(run)` /
//! `validation_state(run)`; ADR-0106 D5 — "`project(run, envelope_view)`,
//! never stored counters"). Every member here is a **pure fold of the
//! durable prefix** — a view the INV-9 rebuild-equality check replays and a
//! checkpoint reads; nothing the envelope knows lives only in process
//! memory.

use hh_ledger::event::EventEnvelope;
use hh_wire::json::Json;

use crate::loops::LoopState;
use crate::output::ValidationState;

/// The `envelope_view` projection — what `project(run, envelope_view)`
/// materializes: the folded counters, streaks, ladder position, format
/// failures, open scopes and the stop-barrier state. The `view_hash` of this
/// canonical record is the INV-9 comparison term.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EnvelopeView {
    /// `control.decision` rows folded (`decision_count`).
    pub decisions: u64,
    /// `control.retry.scheduled` rows (`retries` postings — INV-6).
    pub retries_scheduled: u64,
    /// `control.timeout.fired` rows.
    pub timeouts_fired: u64,
    /// `control.loop.detected` rows (the ladder position).
    pub loop_detections: u64,
    /// `control.output.rejected` rows (`format_failures_running`).
    pub output_rejections: u64,
    /// `control.invariant.violated` rows.
    pub invariant_violations: u64,
    /// `control.budget.exceeded` rows (exhaustion crossings).
    pub budget_exceeded: u64,
    /// The effect ids still open (`intended`/`committed` minus terminals).
    pub open_effects: Vec<String>,
    /// The model calls still open.
    pub open_model_calls: Vec<String>,
    /// Whether the stop barrier has engaged (`control.decision{stop}`).
    pub barrier_engaged: bool,
    /// The recorded stop reason, if the barrier engaged (the `reason`
    /// member of the stop `control.decision`).
    pub stop_reason: Option<Json>,
    /// Whether the run finished (`lifecycle.run.finished`).
    pub run_finished: bool,
    /// The loop fold.
    pub loop_state: LoopState,
    /// The validation fold.
    pub validation_state: ValidationState,
}

/// Fold the `envelope_view` over the durable prefix (INV-9: a fresh fold of
/// the same prefix yields an equal view — [`crate::invariants::rebuild_equal`]
/// compares the canonical forms).
pub fn fold_envelope_view(events: &[EventEnvelope]) -> EnvelopeView {
    let mut v = EnvelopeView {
        loop_state: crate::loops::fold(events),
        validation_state: crate::output::fold(events),
        ..Default::default()
    };
    let mut open_effects: Vec<String> = vec![];
    let mut open_calls: Vec<String> = vec![];
    for ev in events {
        match ev.class.as_str() {
            "control.decision" => {
                v.decisions += 1;
                if ev.payload.get("kind").and_then(Json::as_str) == Some("stop") {
                    v.barrier_engaged = true;
                    v.stop_reason = ev.payload.get("reason").cloned();
                }
            }
            "control.retry.scheduled" => v.retries_scheduled += 1,
            "control.timeout.fired" => v.timeouts_fired += 1,
            "control.loop.detected" => v.loop_detections += 1,
            "control.output.rejected" => v.output_rejections += 1,
            "control.invariant.violated" => v.invariant_violations += 1,
            "control.budget.exceeded" => v.budget_exceeded += 1,
            "action.effect.intended" | "action.effect.committed" => {
                if let Some(id) = &ev.scope.effect_id {
                    if !open_effects.contains(id) {
                        open_effects.push(id.clone());
                    }
                }
            }
            "action.effect.observed"
            | "action.effect.refused"
            | "action.effect.unknown"
            | "action.effect.abandoned" => {
                if let Some(id) = &ev.scope.effect_id {
                    open_effects.retain(|x| x != id);
                }
            }
            "model.call.requested" => {
                if let Some(id) = &ev.scope.model_call_id {
                    if !open_calls.contains(id) {
                        open_calls.push(id.clone());
                    }
                }
            }
            "model.call.completed" | "model.call.failed" => {
                if let Some(id) = &ev.scope.model_call_id {
                    open_calls.retain(|x| x != id);
                }
            }
            "lifecycle.run.finished" => v.run_finished = true,
            _ => {}
        }
    }
    v.open_effects = open_effects;
    v.open_model_calls = open_calls;
    v
}

/// The `envelope_view`'s canonical record — `view_hash` inputs must be
/// canonical bytes (ADR-0029 — one canonicalizer, CC1).
pub fn envelope_view_json(v: &EnvelopeView) -> Json {
    Json::obj([
        ("decisions", Json::Int(v.decisions as i64)),
        ("retries_scheduled", Json::Int(v.retries_scheduled as i64)),
        ("timeouts_fired", Json::Int(v.timeouts_fired as i64)),
        ("loop_detections", Json::Int(v.loop_detections as i64)),
        ("output_rejections", Json::Int(v.output_rejections as i64)),
        (
            "invariant_violations",
            Json::Int(v.invariant_violations as i64),
        ),
        ("budget_exceeded", Json::Int(v.budget_exceeded as i64)),
        (
            "open_effects",
            Json::Arr(v.open_effects.iter().map(Json::str).collect()),
        ),
        (
            "open_model_calls",
            Json::Arr(v.open_model_calls.iter().map(Json::str).collect()),
        ),
        ("barrier_engaged", Json::Bool(v.barrier_engaged)),
        ("stop_reason", v.stop_reason.clone().unwrap_or(Json::Null)),
        ("run_finished", Json::Bool(v.run_finished)),
        (
            "loop_state",
            Json::obj([
                (
                    "ladder_position",
                    Json::Int(v.loop_state.ladder_position as i64),
                ),
                ("error_streak", Json::Int(v.loop_state.error_streak as i64)),
                (
                    "monologue_streak",
                    Json::Int(v.loop_state.monologue_streak as i64),
                ),
            ]),
        ),
        (
            "validation_state",
            Json::obj([(
                "format_failures_running",
                Json::Int(v.validation_state.format_failures_running as i64),
            )]),
        ),
    ])
}

/// `view_hash` — the INV-9 term (`H("envelope_view" ∥ canonical(view))`).
pub fn envelope_view_hash(v: &EnvelopeView) -> String {
    hh_wire::sha256::sha256_hex(
        format!(
            "envelope_view\u{0}{}",
            envelope_view_json(v).to_canonical_string()
        )
        .as_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_ledger::classes::Durability;
    use hh_ledger::event::{EventPlane, Producer, Scope};
    use hh_ledger::manifest::{ObservabilityLevel, ParticipantClass};

    fn ev(seq: u64, class: &str, effect: Option<&str>) -> EventEnvelope {
        EventEnvelope {
            event_id: format!("e{seq}"),
            run_id: "r".into(),
            seq,
            ts: "t".into(),
            hlc: None,
            plane: EventPlane::Action,
            class: class.into(),
            schema_version: 1,
            producer: Producer::kernel("t"),
            participant_class: ParticipantClass::Native,
            observability_level: [ObservabilityLevel::Events].into_iter().collect(),
            durability: Durability::Ledger,
            scope: Scope {
                turn_id: None,
                model_call_id: None,
                tool_call_id: None,
                effect_id: effect.map(String::from),
                child_run_id: None,
                branch_id: None,
            },
            lease_generation: 1,
            parent_event_id: "root".into(),
            causes: vec![],
            refs: vec![],
            ir_refs: vec![],
            surface_ids: Default::default(),
            provenance: None,
            payload: Json::Null,
            prev_hash: "h".into(),
            hash: "h".into(),
        }
    }

    #[test]
    fn the_view_folds_counters_and_open_scopes() {
        let events = vec![
            ev(0, "action.effect.intended", Some("e1")),
            ev(1, "action.effect.intended", Some("e2")),
            ev(2, "action.effect.observed", Some("e1")),
            ev(3, "control.retry.scheduled", None),
            ev(4, "control.decision", None),
        ];
        let v = fold_envelope_view(&events);
        assert_eq!(v.decisions, 1);
        assert_eq!(v.retries_scheduled, 1);
        assert_eq!(v.open_effects, vec!["e2".to_string()]);
        assert!(!v.barrier_engaged);
    }

    #[test]
    fn inv9_rebuild_equality_holds_on_a_replayed_fold() {
        let events = vec![
            ev(0, "control.decision", None),
            ev(1, "control.output.rejected", None),
        ];
        let a = fold_envelope_view(&events);
        let b = fold_envelope_view(&events);
        assert_eq!(envelope_view_hash(&a), envelope_view_hash(&b));
        assert!(crate::invariants::rebuild_equal(
            &envelope_view_json(&a),
            &envelope_view_json(&b)
        ));
    }
}
