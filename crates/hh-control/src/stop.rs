//! The stop protocol (§5e.2; ADR-0106 D8): **a barrier followed by a drain,
//! never an exception**. The sequence is fixed —
//!
//! 1. append `control.decision{kind: stop, reason, decider, triggered_by}`
//!    — from here no new `action.effect.committed` or
//!    `model.call.requested` except declared `grace` calls and drain
//!    operations (INV-3);
//! 2. cancel children (`control.subagent.cancelled{reason: parent_stop}` —
//!    C1/Stage-4 machinery; at Stage 1 the step is vacuous);
//! 3. resolve every pending permission `cancelled` (ACP obligation);
//! 4. let in-flight effects reach a terminal or `unknown` (`expire` at
//!    deadline — the envelope's `TimeoutPolicy` derivation);
//! 5. run the declared `grace` call if any (ADR-0040 E3);
//! 6. release reservations;
//! 7. append `lifecycle.turn.finished{stop_reason}` /
//!    `lifecycle.run.finished{status, stop_reason, outcome_class,
//!    unresolved_effects[], drain_report_ref}` — only then is `idle` lowered
//!    to hosting surfaces (durable-before-visible).
//!
//! `DrainTimeout` ⇒ every still-open effect records `unknown{cause:
//! drain_timeout}` and the run finishes
//! `infrastructure_failure{drain_timeout}` with the **original** reason in
//! `causes[]` (the `error_class` member is a `KernelInfraCause` spelling —
//! CF-479).

use std::collections::{BTreeMap, BTreeSet};

use hh_ledger::event::EventEnvelope;
use hh_ontology::control::{InfraError, InfraErrorFamily, KernelInfraCause, StopReason};
use hh_wire::json::Json;

/// `DrainReport` — `stop()`'s output record (the
/// `lifecycle.run.finished.drain_report_ref` target).
#[derive(Debug, Clone, PartialEq)]
pub struct DrainReport {
    /// The admitted stop reason (the `control.decision{stop}.reason`).
    pub reason: StopReason,
    /// Children cancelled (`control.subagent.cancelled{parent_stop}` ids —
    /// Stage-4; `[]` at Stage 1).
    pub children_cancelled: Vec<String>,
    /// Pending permissions resolved `cancelled`.
    pub permissions_cancelled: Vec<String>,
    /// Effects that reached a terminal during the drain.
    pub settled: Vec<String>,
    /// Effects still open at `drain_deadline` — each records
    /// `unknown{cause: drain_timeout}`.
    pub unknown_recorded: Vec<String>,
    /// Declared `grace` calls that ran.
    pub grace_calls: Vec<String>,
    /// Reservations released.
    pub reservations_released: Vec<String>,
    /// Whether the drain completed inside its bound.
    pub completed: bool,
    /// The terminal `StopReason` the run finishes with — `reason` unless
    /// `drain_timeout` fired, in which case
    /// `infrastructure_failure{drain_timeout}` and `reason` rides `causes[]`.
    pub final_reason: StopReason,
}

impl DrainReport {
    /// Canonical JSON (the `drain_report_ref` artifact's body — the driver
    /// content-addresses it).
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("reason", self.reason.to_json()),
            (
                "children_cancelled",
                Json::Arr(self.children_cancelled.iter().map(Json::str).collect()),
            ),
            (
                "permissions_cancelled",
                Json::Arr(self.permissions_cancelled.iter().map(Json::str).collect()),
            ),
            (
                "settled",
                Json::Arr(self.settled.iter().map(Json::str).collect()),
            ),
            (
                "unknown_recorded",
                Json::Arr(self.unknown_recorded.iter().map(Json::str).collect()),
            ),
            (
                "grace_calls",
                Json::Arr(self.grace_calls.iter().map(Json::str).collect()),
            ),
            (
                "reservations_released",
                Json::Arr(self.reservations_released.iter().map(Json::str).collect()),
            ),
            ("completed", Json::Bool(self.completed)),
            ("final_reason", self.final_reason.to_json()),
        ])
    }
}

/// The barrier check — a `control.decision{kind: stop}` has been appended
/// *and stands* (from its seq, INV-3 forbids new `committed`/`requested` —
/// enforced by [`crate::invariants`]). S3.10: a `verification.gate.
/// evaluated{verdict: hold}` after the stop decision is the durable release
/// — the completion gate refused the completion, so the loop re-opens for
/// the strategy's hold→repair→re-propose (§5f.2; a refused `stop` was never
/// a barrier the run could drain behind). The next admitted `stop`
/// decision re-engages the barrier; a `pass`/`veto` leaves it engaged.
pub fn barrier_engaged(events: &[EventEnvelope]) -> bool {
    let mut barrier = false;
    for e in events {
        if e.class == "control.decision"
            && e.payload.get("kind").and_then(Json::as_str) == Some("stop")
        {
            barrier = true;
        } else if e.class == "verification.gate.evaluated"
            && e.payload.get("verdict").and_then(Json::as_str) == Some("hold")
        {
            barrier = false;
        }
    }
    barrier
}

/// Whether a new `action.effect.committed`/`model.call.requested` is
/// admissible under the barrier — only a declared `grace` call while the
/// drain is open (INV-3's exception), never a fresh step.
pub fn admit_post_barrier_call(events: &[EventEnvelope], is_declared_grace: bool) -> bool {
    !barrier_engaged(events) || is_declared_grace
}

/// The effects still open when the barrier engaged (the drain's work set —
/// `action.effect.intended`/`committed` minus every effect with a terminal).
pub fn in_flight(events: &[EventEnvelope]) -> BTreeSet<String> {
    let mut open = BTreeSet::new();
    for ev in events {
        match ev.class.as_str() {
            "action.effect.intended" | "action.effect.committed" => {
                if let Some(id) = &ev.scope.effect_id {
                    open.insert(id.clone());
                }
            }
            "action.effect.observed"
            | "action.effect.refused"
            | "action.effect.unknown"
            | "action.effect.abandoned" => {
                if let Some(id) = &ev.scope.effect_id {
                    open.remove(id);
                }
            }
            _ => {}
        }
    }
    open
}

/// The permissions still pending — sourced from the durable
/// `security.permission.pending` owed-decision rows (S1.23; `requested` is the
/// prompt *rendering* fact, never the owed-decision source). A *final*
/// `security.permission.decided` (`allow`/`deny` — an `ask` verdict is the
/// one that opened the pending) resolves its `permission_id`; a refusal/
/// `unknown` terminal on every attached `effect_id` resolves the pending
/// `cancelled` (§5g.7 §5 — never `unknown`).
pub fn pending_permissions(events: &[EventEnvelope]) -> BTreeSet<String> {
    // `permission_id → attached effect_ids` — coalesced pendings merge.
    let mut open: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut decided = BTreeSet::new();
    let mut terminated = BTreeSet::new();
    for ev in events {
        match ev.class.as_str() {
            "security.permission.pending" => {
                if let Some(id) = ev.payload.get("permission_id").and_then(Json::as_str) {
                    let row = open.entry(id.to_string()).or_default();
                    if let Some(e) = ev
                        .scope
                        .effect_id
                        .as_deref()
                        .or_else(|| ev.payload.get("effect_id").and_then(Json::as_str))
                    {
                        row.insert(e.to_string());
                    }
                    if let Some(Json::Arr(ids)) = ev.payload.get("effect_ids") {
                        row.extend(ids.iter().filter_map(Json::as_str).map(str::to_string));
                    }
                }
            }
            "security.permission.decided" => {
                let final_verdict = ev
                    .payload
                    .get("decision")
                    .and_then(Json::as_str)
                    .is_some_and(|d| d == "allow" || d == "deny");
                if final_verdict {
                    if let Some(id) = ev.payload.get("permission_id").and_then(Json::as_str) {
                        decided.insert(id.to_string());
                    }
                }
            }
            "action.effect.refused" | "action.effect.unknown" => {
                if let Some(e) = ev
                    .scope
                    .effect_id
                    .as_deref()
                    .or_else(|| ev.payload.get("effect_id").and_then(Json::as_str))
                {
                    terminated.insert(e.to_string());
                }
            }
            _ => {}
        }
    }
    open.retain(|id, effects| {
        !decided.contains(id)
            && (effects.is_empty() || !effects.iter().all(|e| terminated.contains(e)))
    });
    open.keys().cloned().collect()
}

/// `stop(run, reason, triggered_by)` → `DrainReport` — evaluates the drain
/// against the durable prefix the caller extended to the drain bound:
/// `events` is the prefix *as of evaluation*, `drain_deadline_ms` the
/// `TimeoutPolicy`-derived bound, `now_ms` the logical clock. When `now_ms
/// ≥ drain_deadline_ms` with effects still open the report records
/// `drain_timeout`: `unknown{cause: drain_timeout}` per effect and
/// `final_reason = infrastructure_failure{drain_timeout}` — the original
/// `reason` is preserved on the `control.decision{stop}` row and in
/// `causes[]` of the terminal rows.
pub fn assess_drain(
    events: &[EventEnvelope],
    reason: &StopReason,
    drain_deadline_ms: u64,
    now_ms: u64,
    grace_calls_run: &[String],
    reservations_released: &[String],
) -> DrainReport {
    let open = in_flight(events);
    let timed_out = now_ms >= drain_deadline_ms && !open.is_empty();
    let unknown: Vec<String> = if timed_out {
        open.iter().cloned().collect()
    } else {
        vec![]
    };
    let settled: Vec<String> = {
        let mut s = vec![];
        let mut seen = BTreeSet::new();
        for ev in events {
            if matches!(
                ev.class.as_str(),
                "action.effect.observed"
                    | "action.effect.refused"
                    | "action.effect.unknown"
                    | "action.effect.abandoned"
            ) {
                if let Some(id) = &ev.scope.effect_id {
                    if seen.insert(id.clone()) {
                        s.push(id.clone());
                    }
                }
            }
        }
        s
    };
    DrainReport {
        final_reason: if timed_out {
            StopReason::InfrastructureFailure {
                error_class: InfraError {
                    family: InfraErrorFamily::Kernel,
                    class: KernelInfraCause::DrainTimeout.as_str().into(),
                },
            }
        } else {
            reason.clone()
        },
        reason: reason.clone(),
        children_cancelled: vec![],
        permissions_cancelled: pending_permissions(events).iter().cloned().collect(),
        settled,
        unknown_recorded: unknown,
        grace_calls: grace_calls_run.to_vec(),
        reservations_released: reservations_released.to_vec(),
        completed: !timed_out,
    }
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
                component_call_id: None,
                branch_id: None,
            },
            lease_generation: 1,
            parent_event_id: "root".into(),
            causes: vec![],
            refs: vec![],
            ir_refs: vec![],
            surface_ids: Default::default(),
            provenance: None,
            payload: if class == "control.decision" {
                Json::obj([("kind", Json::str("stop"))])
            } else {
                Json::Null
            },
            prev_hash: "h".into(),
            hash: "h".into(),
        }
    }

    /// A permission-class envelope — `payload` carries the owed-decision
    /// members (`permission_id`, `decision`, …); `effect` sets the scope.
    fn perm_ev(seq: u64, class: &str, effect: Option<&str>, payload: Json) -> EventEnvelope {
        let mut e = ev(seq, class, effect);
        e.plane = EventPlane::Security;
        e.payload = payload;
        e
    }

    #[test]
    fn pending_permissions_fold_durable_rows_only() {
        let pid = Json::obj([("permission_id", Json::str("perm-1"))]);
        // An `ask` verdict opens the pending — it must NOT resolve it.
        let events = vec![
            perm_ev(0, "security.permission.decided", Some("e1"), {
                let mut m = match pid.clone() {
                    Json::Obj(m) => m,
                    _ => unreachable!(),
                };
                m.insert("decision".to_string(), Json::str("ask"));
                Json::Obj(m)
            }),
            perm_ev(1, "security.permission.pending", Some("e1"), pid.clone()),
        ];
        assert_eq!(pending_permissions(&events), ["perm-1".to_string()].into());

        // A final `deny` resolves it.
        let mut events2 = events.clone();
        events2.push(perm_ev(2, "security.permission.decided", Some("e1"), {
            let mut m = match pid {
                Json::Obj(m) => m,
                _ => unreachable!(),
            };
            m.insert("decision".to_string(), Json::str("deny"));
            Json::Obj(m)
        }));
        assert!(pending_permissions(&events2).is_empty());
    }

    #[test]
    fn pending_permissions_cancelled_when_every_effect_terminates() {
        let events = vec![
            perm_ev(
                0,
                "security.permission.pending",
                Some("e1"),
                Json::obj([("permission_id", Json::str("perm-1"))]),
            ),
            ev(1, "action.effect.refused", Some("e1")),
        ];
        // The refused effect resolves the pending `cancelled` — never owed.
        assert!(pending_permissions(&events).is_empty());
        // A pending whose effect is still open stays owed.
        let still_open = vec![perm_ev(
            0,
            "security.permission.pending",
            Some("e2"),
            Json::obj([("permission_id", Json::str("perm-2"))]),
        )];
        assert_eq!(
            pending_permissions(&still_open),
            ["perm-2".to_string()].into()
        );
    }

    #[test]
    fn a_completed_drain_keeps_the_reason() {
        let events = vec![
            ev(0, "action.effect.intended", Some("e1")),
            ev(1, "control.decision", None),
            ev(2, "action.effect.observed", Some("e1")),
        ];
        let r = assess_drain(
            &events,
            &StopReason::Completed,
            1_000,
            10,
            &[],
            &["res-1".into()],
        );
        assert!(r.completed);
        assert_eq!(r.final_reason, StopReason::Completed);
        assert_eq!(r.settled, vec!["e1".to_string()]);
    }

    #[test]
    fn a_drain_timeout_records_unknown_and_swaps_the_reason() {
        let events = vec![
            ev(0, "action.effect.committed", Some("e1")),
            ev(1, "control.decision", None),
        ];
        let r = assess_drain(&events, &StopReason::Completed, 1_000, 2_000, &[], &[]);
        assert!(!r.completed);
        assert_eq!(r.unknown_recorded, vec!["e1".to_string()]);
        match &r.final_reason {
            StopReason::InfrastructureFailure { error_class } => {
                assert_eq!(error_class.family, InfraErrorFamily::Kernel);
                assert_eq!(error_class.class, "drain_timeout");
            }
            other => panic!("{other:?}"),
        }
        // …and the original reason survives on the report (`causes[]`).
        assert_eq!(r.reason, StopReason::Completed);
    }

    #[test]
    fn the_barrier_blocks_new_work_but_admits_grace() {
        let events = vec![ev(0, "control.decision", None)];
        assert!(barrier_engaged(&events));
        assert!(!admit_post_barrier_call(&events, false));
        assert!(admit_post_barrier_call(&events, true));
        assert!(admit_post_barrier_call(&[], false));
    }
}
