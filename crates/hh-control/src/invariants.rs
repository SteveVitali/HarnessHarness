//! INV-1…INV-9 — the mandatory state-invariant set (§5e.2; ADR-0108 D3).
//! Each invariant is a **pure predicate over ledger records** — the fold
//! takes the durable prefix plus the derived data the rule reads (deadlines,
//! the budget-conservation view) and returns violations with evidence refs.
//! A violation drives `control.invariant.violated{invariant_id,
//! evidence_refs[], detected_at_guard}` → `stop{invariant_violation}` →
//! outcome `infrastructure_failure` → `security.audit.checkpoint{kind:
//! quarantine}`; the run is not scored.
//!
//! | id | predicate (pure over the durable prefix) |
//! |---|---|
//! | INV-1 | every opened scope closes by exactly one terminal ≤ its deadline (the envelope force-closes past-deadline scopes — the *check* flags a scope still open past `now > deadline`) |
//! | INV-2 | no dangling intent — every `action.effect.intended` has a terminal or `unknown` visible to the model |
//! | INV-3 | stop is a barrier — no `action.effect.committed`/`model.call.requested` after the `control.decision{stop}` seq (declared `grace` calls + drain operations excepted) |
//! | INV-4 | conservation — Σ children `consumed + reserved` ≤ each ancestor ceiling at every charge |
//! | INV-5 | attempt identity — `attempt_no` strictly increases per scope; ≤ 1 `committed` per `(effect_id, attempt_no)`; idempotency key invariant across attempts |
//! | INV-6 | every feedback path bounded — `control.retry.scheduled` posts ≤ the `retries` ceiling (validator/compaction/spawn counters likewise bounded by their ceilings) |
//! | INV-7 | the envelope never widens — no envelope-produced `allow` permission decision; no loosening `control.budget.amended` produced by the envelope |
//! | INV-8 | no automatic redispatch of `irreversible`; no redispatch of `unknown` without probe/idempotent class |
//! | INV-9 | rebuild equality at turn boundaries — `effect_ledger` and budget `remaining` views fold identically from the recorded prefix (checked by replaying the fold) |

use std::collections::{BTreeMap, BTreeSet};

use hh_ledger::event::EventEnvelope;
use hh_ontology::control::InvariantId;
use hh_wire::json::Json;

use crate::vocab::GuardPoint;

/// A fired invariant — `control.invariant.violated`'s record.
#[derive(Debug, Clone, PartialEq)]
pub struct InvariantViolation {
    /// The violated invariant.
    pub invariant_id: InvariantId,
    /// The evidence event ids/refs.
    pub evidence_refs: Vec<String>,
    /// The guard point that detected it.
    pub detected_at_guard: GuardPoint,
    /// A one-line machine-spellable detail (an enum-ish tag, never `Text`).
    pub detail: String,
}

/// `QuarantineReport` — the `security.audit.checkpoint{kind: quarantine}`
/// record the driver appends after `stop{invariant_violation}` (the run is
/// not scored).
#[derive(Debug, Clone, PartialEq)]
pub struct QuarantineReport {
    /// The violation that quarantined the run.
    pub violation: InvariantViolation,
    /// The checkpoint payload (canonical).
    pub payload: Json,
}

/// Evaluate every declared invariant over the durable prefix at `guard`
/// (`input` carries what the predicate cannot fold alone — see
/// [`InvariantInput`]). Deterministic: two evaluations at the same `seq`
/// yield the same set (AC-F2-10).
pub fn check(
    events: &[EventEnvelope],
    guard: GuardPoint,
    input: &InvariantInput,
) -> Vec<InvariantViolation> {
    let mut out = vec![];
    inv1(events, guard, input, &mut out);
    inv2(events, guard, &mut out);
    inv3(events, guard, &mut out);
    inv4(events, guard, input, &mut out);
    inv5(events, guard, &mut out);
    inv6(events, guard, input, &mut out);
    inv7(events, guard, &mut out);
    inv8(events, guard, &mut out);
    out
}

/// What the predicates cannot fold from events alone — the caller supplies
/// it (deadlines the envelope derived, the budget-conservation snapshot,
/// the `retries` ceiling, the logical `now`).
#[derive(Debug, Clone, Default)]
pub struct InvariantInput {
    /// `scope_id → deadline_ms` the `TimeoutPolicy` derivation assigned.
    pub deadlines: BTreeMap<String, u64>,
    /// The logical now (ms) — injectable, never `Instant::now`.
    pub now_ms: u64,
    /// The budget-conservation view: `ancestor → (consumed + reserved,
    /// ceiling)` pairs the last charge was checked against (the budget
    /// crate's fold — the envelope reads, never recomputes).
    pub conservation: Vec<ConservationRow>,
    /// The `retries` hard ceiling (INV-6's bound).
    pub retries_ceiling: u64,
    /// `effect_id → effect_class` for INV-8 (`irreversible` never
    /// redispatches).
    pub effect_classes: BTreeMap<String, String>,
    /// `(effect_id → idempotency_key)` per attempt for INV-5.
    pub idempotency_keys: BTreeMap<String, String>,
    /// The scope ids still open (the caller's open-scope fold — INV-1 fires
    /// only on a *past-deadline and still open* scope).
    pub open_scope_ids: BTreeSet<String>,
}

/// One conservation row: `(ancestor, consumed + reserved, ceiling)`.
#[derive(Debug, Clone, PartialEq)]
pub struct ConservationRow {
    /// The ancestor budget node.
    pub ancestor: String,
    /// `consumed + reserved` charged under the ancestor.
    pub total: i64,
    /// The ancestor's ceiling.
    pub ceiling: i64,
    /// The charge's evidence ref.
    pub evidence_ref: String,
}

/// The seq of the standing `control.decision{kind: stop}` (the INV-3
/// barrier). S3.10: a `verification.gate.evaluated{verdict: hold}` after a
/// stop decision releases the barrier — the completion gate refused the
/// completion, so the hold→repair→re-propose loop may open new model calls
/// and effects until the next admitted `stop` re-engages it
/// ([`crate::stop::barrier_engaged`] shares the rule).
fn stop_barrier_seq(events: &[EventEnvelope]) -> Option<u64> {
    let mut barrier: Option<u64> = None;
    for e in events {
        if e.class == "control.decision"
            && e.payload.get("kind").and_then(Json::as_str) == Some("stop")
        {
            barrier = Some(e.seq);
        } else if e.class == "verification.gate.evaluated"
            && e.payload.get("verdict").and_then(Json::as_str) == Some("hold")
        {
            barrier = None;
        }
    }
    barrier
}

/// INV-1 — an opened scope still open with `now > deadline` is a violation
/// (the envelope force-closes it; the predicate flags the breach — the
/// close-by is the *repair*, the flag is the evidence).
fn inv1(
    _events: &[EventEnvelope],
    guard: GuardPoint,
    input: &InvariantInput,
    out: &mut Vec<InvariantViolation>,
) {
    for (scope_id, dl) in &input.deadlines {
        if input.now_ms > *dl && input.open_scope_ids.contains(scope_id) {
            out.push(InvariantViolation {
                invariant_id: InvariantId::Inv1,
                evidence_refs: vec![scope_id.clone()],
                detected_at_guard: guard,
                detail: format!("scope_past_deadline{{{scope_id}}}"),
            });
        }
    }
}

/// INV-2 — `action.effect.intended` with no terminal/`unknown` is a dangling
/// intent (evaluated at turn boundary — G-DECIDE/G-POST-EFFECT pass the
/// still-open intents; at those guards an open intent that has not yet been
/// *dispatched* is not yet dangling — the predicate fires on intents that
/// remain open while the model has already been shown a settled batch).
fn inv2(events: &[EventEnvelope], guard: GuardPoint, out: &mut Vec<InvariantViolation>) {
    let mut intended: BTreeSet<String> = BTreeSet::new();
    let mut settled: BTreeSet<String> = BTreeSet::new();
    let mut delivered_settlement = false;
    for ev in events {
        match ev.class.as_str() {
            "action.effect.intended" => {
                if let Some(id) = &ev.scope.effect_id {
                    intended.insert(id.clone());
                }
            }
            "action.effect.observed"
            | "action.effect.refused"
            | "action.effect.unknown"
            | "action.effect.abandoned" => {
                if let Some(id) = &ev.scope.effect_id {
                    settled.insert(id.clone());
                }
            }
            // An `effects_settled` decision means the model saw the batch's
            // terminals — any *other* intended effect still open is dangling.
            "control.decision"
                if ev.payload.get("kind").and_then(Json::as_str) == Some("propose") =>
            {
                delivered_settlement = true;
            }
            _ => {}
        }
    }
    if delivered_settlement {
        for id in intended.difference(&settled) {
            out.push(InvariantViolation {
                invariant_id: InvariantId::Inv2,
                evidence_refs: vec![id.clone()],
                detected_at_guard: guard,
                detail: format!("dangling_intent{{{id}}}"),
            });
        }
    }
}

/// INV-3 — a `action.effect.committed` or `model.call.requested` after the
/// stop barrier (grace calls are declared by id — a `grace` flag on the
/// call's payload excepts it; drain operations ride `control.*` classes and
/// are not scope openers).
fn inv3(events: &[EventEnvelope], guard: GuardPoint, out: &mut Vec<InvariantViolation>) {
    let Some(barrier) = stop_barrier_seq(events) else {
        return;
    };
    for ev in events.iter().filter(|e| e.seq > barrier) {
        match ev.class.as_str() {
            "action.effect.committed" | "model.call.requested" => {
                let grace = ev
                    .payload
                    .get("grace")
                    .map(|g| matches!(g, Json::Bool(true)))
                    .unwrap_or(false);
                if !grace {
                    out.push(InvariantViolation {
                        invariant_id: InvariantId::Inv3,
                        evidence_refs: vec![ev.event_id.clone()],
                        detected_at_guard: guard,
                        detail: format!("post_barrier_open{{{}}}", ev.class),
                    });
                }
            }
            _ => {}
        }
    }
}

/// INV-4 — a conservation row over its ceiling.
fn inv4(
    _events: &[EventEnvelope],
    guard: GuardPoint,
    input: &InvariantInput,
    out: &mut Vec<InvariantViolation>,
) {
    for row in &input.conservation {
        if row.total > row.ceiling {
            out.push(InvariantViolation {
                invariant_id: InvariantId::Inv4,
                evidence_refs: vec![row.evidence_ref.clone()],
                detected_at_guard: guard,
                detail: format!(
                    "conservation_breach{{{}:{}/{}}}",
                    row.ancestor, row.total, row.ceiling
                ),
            });
        }
    }
}

/// INV-5 — attempt identity: `attempt_no` strictly increases per scope;
/// ≤ 1 `committed` per `(effect_id, attempt_no)`; idempotency key invariant.
fn inv5(events: &[EventEnvelope], guard: GuardPoint, out: &mut Vec<InvariantViolation>) {
    let mut last_attempt: BTreeMap<String, u64> = BTreeMap::new();
    let mut committed: BTreeSet<(String, u64)> = BTreeSet::new();
    let mut idem: BTreeMap<String, String> = BTreeMap::new();
    for ev in events {
        let scope_id = [
            ev.scope.model_call_id.as_deref(),
            ev.scope.effect_id.as_deref(),
            ev.scope.tool_call_id.as_deref(),
            ev.scope.child_run_id.as_deref(),
        ]
        .iter()
        .flatten()
        .next()
        .map(|s| s.to_string());
        let attempt = ev.payload.get("attempt_no").and_then(Json::as_int);
        // Only scope *openers* advance the attempt series — a terminal
        // (`completed`/`failed`/`observed`/…) legitimately echoes the
        // attempt it closes, and `control.retry.scheduled` *announces*
        // the next attempt the opener then records.
        let opener = matches!(
            ev.class.as_str(),
            "model.call.requested" | "action.effect.intended" | "action.effect.committed"
        );
        if opener {
            if let (Some(id), Some(a)) = (&scope_id, attempt) {
                let a = a as u64;
                if let Some(prev) = last_attempt.get(id) {
                    if a <= *prev {
                        out.push(InvariantViolation {
                            invariant_id: InvariantId::Inv5,
                            evidence_refs: vec![ev.event_id.clone()],
                            detected_at_guard: guard,
                            detail: format!("attempt_not_increasing{{{}:{a}≤{prev}}}", id),
                        });
                    }
                }
                last_attempt.insert(id.clone(), a.max(*last_attempt.get(id).unwrap_or(&0)));
            }
        }
        if ev.class == "action.effect.committed" {
            if let (Some(id), Some(a)) = (&scope_id, attempt) {
                if !committed.insert((id.clone(), a as u64)) {
                    out.push(InvariantViolation {
                        invariant_id: InvariantId::Inv5,
                        evidence_refs: vec![ev.event_id.clone()],
                        detected_at_guard: guard,
                        detail: format!("duplicate_commit{{{}:{a}}}", id),
                    });
                }
            }
            if let Some(id) = &scope_id {
                if let Some(k) = ev.payload.get("idempotency_key").and_then(Json::as_str) {
                    match idem.get(id) {
                        Some(prev) if prev != k => {
                            out.push(InvariantViolation {
                                invariant_id: InvariantId::Inv5,
                                evidence_refs: vec![ev.event_id.clone()],
                                detected_at_guard: guard,
                                detail: format!("idempotency_key_drift{{{id}}}"),
                            });
                        }
                        None => {
                            idem.insert(id.clone(), k.to_string());
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

/// INV-6 — `control.retry.scheduled` postings beyond the `retries` ceiling.
fn inv6(
    events: &[EventEnvelope],
    guard: GuardPoint,
    input: &InvariantInput,
    out: &mut Vec<InvariantViolation>,
) {
    let scheduled = events
        .iter()
        .filter(|e| e.class == "control.retry.scheduled")
        .count() as u64;
    if scheduled > input.retries_ceiling && input.retries_ceiling > 0 {
        out.push(InvariantViolation {
            invariant_id: InvariantId::Inv6,
            evidence_refs: events
                .iter()
                .filter(|e| e.class == "control.retry.scheduled")
                .map(|e| e.event_id.clone())
                .collect(),
            detected_at_guard: guard,
            detail: format!(
                "unbounded_feedback{{retries:{scheduled}>{}}}",
                input.retries_ceiling
            ),
        });
    }
}

/// INV-7 — an envelope-produced `security.permission.decided{allow}` or a
/// loosening `control.budget.amended` produced by this component is a
/// widening the envelope may never emit.
fn inv7(events: &[EventEnvelope], guard: GuardPoint, out: &mut Vec<InvariantViolation>) {
    for ev in events {
        let ours = ev.producer.component_variant_ref == crate::events::COMPONENT;
        match ev.class.as_str() {
            "security.permission.decided" => {
                if ours && ev.payload.get("decision").and_then(Json::as_str) == Some("allow") {
                    out.push(InvariantViolation {
                        invariant_id: InvariantId::Inv7,
                        evidence_refs: vec![ev.event_id.clone()],
                        detected_at_guard: guard,
                        detail: "envelope_granted_permission".into(),
                    });
                }
            }
            "control.budget.amended" => {
                if ours {
                    // Any envelope-emitted amend is a widening suspect —
                    // `consumed/reserved/released/exceeded` are the only
                    // budget events the envelope may produce.
                    out.push(InvariantViolation {
                        invariant_id: InvariantId::Inv7,
                        evidence_refs: vec![ev.event_id.clone()],
                        detected_at_guard: guard,
                        detail: "envelope_amended_budget".into(),
                    });
                }
            }
            _ => {}
        }
    }
}

/// INV-8 — a second `action.effect.committed` (a redispatch) for an
/// `irreversible` effect, or for an effect whose last terminal was
/// `unknown` without an intervening probe.
fn inv8(events: &[EventEnvelope], guard: GuardPoint, out: &mut Vec<InvariantViolation>) {
    let mut classes: BTreeMap<String, String> = BTreeMap::new();
    let mut commits: BTreeMap<String, u32> = BTreeMap::new();
    let mut last_terminal: BTreeMap<String, String> = BTreeMap::new();
    let mut probed: BTreeSet<String> = BTreeSet::new();
    for ev in events {
        let id = match &ev.scope.effect_id {
            Some(i) => i.clone(),
            None => continue,
        };
        match ev.class.as_str() {
            "action.effect.intended" => {
                if let Some(c) = ev.payload.get("effect_class").and_then(Json::as_str) {
                    classes.insert(id.clone(), c.to_string());
                }
            }
            "action.effect.committed" => {
                *commits.entry(id.clone()).or_default() += 1;
                if commits[&id] > 1 {
                    let cls = classes.get(&id).map(String::as_str).unwrap_or("");
                    let unknown = last_terminal.get(&id).map(String::as_str) == Some("unknown");
                    let idem = ev
                        .payload
                        .get("idempotent")
                        .map(|b| matches!(b, Json::Bool(true)))
                        .unwrap_or(false);
                    if cls == "irreversible" || (unknown && !probed.contains(&id) && !idem) {
                        out.push(InvariantViolation {
                            invariant_id: InvariantId::Inv8,
                            evidence_refs: vec![ev.event_id.clone()],
                            detected_at_guard: guard,
                            detail: format!("illegal_redispatch{{{id}}}"),
                        });
                    }
                }
            }
            "action.effect.observed"
            | "action.effect.refused"
            | "action.effect.unknown"
            | "action.effect.abandoned" => {
                last_terminal.insert(
                    id.clone(),
                    ev.class
                        .strip_prefix("action.effect.")
                        .unwrap_or("")
                        .to_string(),
                );
            }
            "action.effect.probed" => {
                probed.insert(id.clone());
            }
            _ => {}
        }
    }
}

/// INV-9's rebuild-equality witness — the recorded `effect_ledger`/
/// `remaining` view equals a fresh fold of the same prefix. The caller
/// folds both sides (the view is a pure fold; equality is the check).
pub fn rebuild_equal(recorded_view: &Json, folded_view: &Json) -> bool {
    recorded_view.to_canonical_string() == folded_view.to_canonical_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_ledger::classes::Durability;
    use hh_ledger::event::{EventPlane, Producer, Scope};
    use hh_ledger::manifest::{ObservabilityLevel, ParticipantClass};

    fn ev(seq: u64, class: &str, effect: Option<&str>, payload: Json) -> EventEnvelope {
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
            payload,
            prev_hash: "h".into(),
            hash: "h".into(),
        }
    }

    #[test]
    fn inv3_flags_a_commit_after_the_stop_barrier() {
        let events = vec![
            ev(
                0,
                "control.decision",
                None,
                Json::obj([("kind", Json::str("stop"))]),
            ),
            ev(
                1,
                "action.effect.committed",
                Some("e1"),
                Json::obj([("attempt_no", Json::Int(1))]),
            ),
        ];
        let vs = check(&events, GuardPoint::PostEffect, &InvariantInput::default());
        assert!(vs.iter().any(|v| v.invariant_id == InvariantId::Inv3));
        // A declared grace call does not violate.
        let g = vec![
            ev(
                0,
                "control.decision",
                None,
                Json::obj([("kind", Json::str("stop"))]),
            ),
            ev(
                1,
                "action.effect.committed",
                Some("e1"),
                Json::obj([("attempt_no", Json::Int(1)), ("grace", Json::Bool(true))]),
            ),
        ];
        let vs2 = check(&g, GuardPoint::PostEffect, &InvariantInput::default());
        assert!(!vs2.iter().any(|v| v.invariant_id == InvariantId::Inv3));
    }

    #[test]
    fn inv5_flags_duplicate_commits_and_nonmonotone_attempts() {
        let events = vec![
            ev(
                0,
                "action.effect.committed",
                Some("e1"),
                Json::obj([("attempt_no", Json::Int(2))]),
            ),
            ev(
                1,
                "action.effect.committed",
                Some("e1"),
                Json::obj([("attempt_no", Json::Int(1))]),
            ),
        ];
        let vs = check(&events, GuardPoint::PostEffect, &InvariantInput::default());
        assert!(vs.iter().any(|v| v.invariant_id == InvariantId::Inv5));
    }

    #[test]
    fn inv7_flags_an_envelope_produced_allow() {
        let mut e = ev(
            0,
            "security.permission.decided",
            None,
            Json::obj([("decision", Json::str("allow"))]),
        );
        e.producer = Producer::kernel(crate::events::COMPONENT);
        let vs = check(&[e], GuardPoint::PreDispatch, &InvariantInput::default());
        assert!(vs.iter().any(|v| v.invariant_id == InvariantId::Inv7));
    }

    #[test]
    fn inv8_flags_irreversible_redispatch() {
        let events = vec![
            ev(
                0,
                "action.effect.intended",
                Some("e1"),
                Json::obj([("effect_class", Json::str("irreversible"))]),
            ),
            ev(
                1,
                "action.effect.committed",
                Some("e1"),
                Json::obj([("attempt_no", Json::Int(1))]),
            ),
            ev(2, "action.effect.observed", Some("e1"), Json::Null),
            ev(
                3,
                "action.effect.committed",
                Some("e1"),
                Json::obj([("attempt_no", Json::Int(2))]),
            ),
        ];
        let vs = check(&events, GuardPoint::PreDispatch, &InvariantInput::default());
        assert!(vs.iter().any(|v| v.invariant_id == InvariantId::Inv8));
    }

    #[test]
    fn inv9_rebuild_equality_is_canonical() {
        let a = Json::obj([("x", Json::Int(1))]);
        let b = hh_wire::json::parse(&a.to_canonical_string()).unwrap();
        assert!(rebuild_equal(&a, &b));
        assert!(!rebuild_equal(&a, &Json::obj([("x", Json::Int(2))])));
    }
}
