//! The deterministic `loop_detector` (§5e.2 `LoopPolicy`; ADR-0108 D1;
//! T-LCD-10). A detector is a registered variant of the `loop_detector`
//! class; at C0 the four deterministic variants run — `exact_repeat`,
//! `no_progress`, `error_streak`, `monologue` (`content_chant`/`judged` are
//! C1, gated by [`LoopDetectorKind::admitted_at_stage1`]).
//!
//! `loop_key = H(capability.semantic_id ∥ canonical(params via
//! SurfaceArgMap))` — never the surface name. The **no-progress predicate**
//! is `same loop_key ∧ same Observation.version_id` (one predicate, one
//! threshold, owned here — CF-229). Detection is a pure fold of the durable
//! prefix; `loop_state(run)` is a materialized view, never a stored counter
//! (ADR-0106 D5). The `response_ladder` is `[nudge, deny, stop]`, each rung
//! once per run — the ladder position is the count of prior
//! `control.loop.detected` rows.

use hh_ledger::event::EventEnvelope;
use hh_ontology::control::{LoopDetectorKind, LoopPattern};
use hh_wire::json::Json;
use hh_wire::sha256::sha256_hex;

use crate::policy::{LadderAction, LoopPolicy};

/// `loop_key = H(semantic_id ∥ canonical(params))` — the semantic key a
/// detector folds (T-LCD-10: never the surface name; the driver computes it
/// over the *parsed* `SurfaceArgMap`, so two byte-different encodings of one
/// call share a key).
pub fn loop_key(semantic_id: &str, canonical_params: &Json) -> String {
    sha256_hex(
        format!(
            "{}\u{0}{}",
            semantic_id,
            canonical_params.to_canonical_string()
        )
        .as_bytes(),
    )
}

/// One detector observation in the fold window.
#[derive(Debug, Clone, PartialEq)]
struct Observation {
    /// The event seq (ordering + evidence).
    seq: u64,
    /// The event id (evidence ref).
    event_id: String,
    /// The loop key when the observation is a proposed/committed call.
    key: Option<String>,
    /// The observation `version_id` (no-progress evidence).
    version_id: Option<String>,
    /// An error-class terminal spelling (error-streak evidence).
    error: Option<String>,
    /// A model turn that produced no `act` (monologue evidence).
    turn_without_act: bool,
}

/// `LoopState` — the materialized envelope view over the detector window
/// (`loop_state(run)`; rebuilt from the durable prefix every guard call —
/// INV-9's rebuild equality covers it).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LoopState {
    /// `detector → rungs already fired this run` (each rung once per run).
    pub fired: std::collections::BTreeMap<String, u32>,
    /// The running `format_failures`-adjacent error streak (exposed for the
    /// view; the verdict uses the window fold).
    pub error_streak: u32,
    /// Consecutive model turns without an `act` decision.
    pub monologue_streak: u32,
    /// Ladder position = number of `control.loop.detected` rows so far.
    pub ladder_position: u32,
}

/// `LoopHit` — a fired detector: which variant, the pattern, the ladder
/// action the policy assigns, and the evidence refs.
#[derive(Debug, Clone, PartialEq)]
pub struct LoopHit {
    /// The detector variant.
    pub detector: LoopDetectorKind,
    /// The detected pattern.
    pub pattern: LoopPattern,
    /// The ladder action (`nudge` | `deny` | `stop`).
    pub action: LadderAction,
    /// The ladder position this hit occupies.
    pub ladder_position: u32,
    /// The evidence event ids.
    pub evidence_refs: Vec<String>,
}

/// `LadderPosition` — where a fresh hit lands on the `response_ladder`
/// (`Some(rung)` or `None` when the ladder is exhausted — the last `stop`
/// rung having fired means the run is already stopping).
pub type LadderPosition = Option<LadderAction>;

/// Fold the durable prefix into `LoopState` (the view the envelope exposes —
/// `loop_state(run)`).
pub fn fold(events: &[EventEnvelope]) -> LoopState {
    let mut st = LoopState::default();
    let mut pending_errors = 0u32;
    let mut pending_monologue = 0u32;
    for ev in events {
        match ev.class.as_str() {
            "control.loop.detected" => {
                st.ladder_position += 1;
                if let Some(d) = ev.payload.get("detector").and_then(Json::as_str) {
                    *st.fired.entry(d.to_string()).or_default() += 1;
                }
                // A fired detector resets the contributing streaks.
                pending_errors = 0;
                pending_monologue = 0;
            }
            "model.call.failed" | "action.effect.unknown" | "action.effect.refused" => {
                pending_errors += 1;
            }
            "action.effect.observed" => {
                if ev
                    .payload
                    .get("status")
                    .and_then(Json::as_str)
                    .map(|s| s == "error")
                    .unwrap_or(false)
                {
                    pending_errors += 1;
                } else {
                    pending_errors = 0;
                }
            }
            "control.decision" => {
                if ev
                    .payload
                    .get("kind")
                    .and_then(Json::as_str)
                    .map(|k| k == "act")
                    .unwrap_or(false)
                {
                    pending_monologue = 0;
                }
            }
            "model.call.completed" => {
                pending_monologue += 1;
            }
            _ => {}
        }
        st.error_streak = pending_errors;
        st.monologue_streak = pending_monologue;
    }
    st
}

/// The rung a fresh hit takes (`response_ladder[ladder_position]` — each
/// rung once per run; `None` once `stop` has fired).
pub fn next_rung(policy: &LoopPolicy, state: &LoopState) -> LadderPosition {
    policy
        .response_ladder
        .get(state.ladder_position as usize)
        .copied()
}

/// Run every C1-admitted deterministic detector over the durable prefix and
/// return the hits in detection order (a pure fold — the same `seq` yields
/// the same hits, AC-F2-10).
///
/// The window is `policy.window_events` trailing *observations* (call-class
/// events — `action.tool.proposed`/`action.effect.*`/`model.call.*`), never
/// wall-time; detector membership is closed by
/// [`LoopDetectorKind::admitted_at_stage1`].
pub fn detect(events: &[EventEnvelope], policy: &LoopPolicy) -> Vec<LoopHit> {
    let state = fold(events);
    let rung = match next_rung(policy, &state) {
        Some(r) => r,
        None => return vec![], // the `stop` rung already fired
    };
    let window = collect_window(events, policy.window_events as usize);
    let mut hits = vec![];
    for detector in [
        LoopDetectorKind::ExactRepeat,
        LoopDetectorKind::NoProgress,
        LoopDetectorKind::ErrorStreak,
        LoopDetectorKind::Monologue,
    ] {
        if !detector.admitted_at_stage1() {
            continue;
        }
        // The ladder is global and consumed one rung per hit — a
        // *window* detector whose predicate still holds re-fires on each
        // evaluation (the `nudge → deny → stop` sequence of AC-R-2.6.2-1
        // is the same pattern persisting, `total calls ≤ threshold + 2`;
        // `next_rung` returns `None` once `stop` has fired, so the ladder
        // bounds the re-fire count itself). A *streak* detector's
        // contributing streak resets on detection (the fold's semantics —
        // only observations *after* the last `control.loop.detected`
        // count toward a fresh streak).
        let last_detect_seq = events
            .iter()
            .filter(|e| e.class == "control.loop.detected")
            .map(|e| e.seq)
            .max();
        let fresh: Vec<Observation> = window
            .iter()
            .filter(|o| last_detect_seq.map(|l| o.seq > l).unwrap_or(true))
            .cloned()
            .collect();
        if let Some(pattern) = match detector {
            LoopDetectorKind::ExactRepeat => exact_repeat(&window, policy),
            LoopDetectorKind::NoProgress => no_progress(&window, policy),
            LoopDetectorKind::ErrorStreak => error_streak(&fresh, policy),
            LoopDetectorKind::Monologue => monologue(&fresh, events, policy),
            _ => None,
        } {
            hits.push(LoopHit {
                detector,
                evidence_refs: pattern.loop_keys.to_vec(),
                pattern,
                action: rung,
                ladder_position: state.ladder_position,
            });
        }
    }
    hits
}

/// Collect the trailing `n` call-class observations (the detector window).
fn collect_window(events: &[EventEnvelope], n: usize) -> Vec<Observation> {
    let mut obs: Vec<Observation> = vec![];
    let mut last_was_act = true;
    for ev in events {
        let o = match ev.class.as_str() {
            // `action.tool.proposed` carries the driver's `loop_key`
            // (`semantic_id ∥ canonical(params)` — data, never args bytes).
            "action.tool.proposed" => Observation {
                seq: ev.seq,
                event_id: ev.event_id.clone(),
                key: ev
                    .payload
                    .get("loop_key")
                    .and_then(Json::as_str)
                    .map(String::from),
                version_id: None,
                error: None,
                turn_without_act: false,
            },
            "action.effect.observed" => Observation {
                seq: ev.seq,
                event_id: ev.event_id.clone(),
                key: ev
                    .payload
                    .get("loop_key")
                    .and_then(Json::as_str)
                    .map(String::from),
                version_id: ev
                    .payload
                    .get("version_id")
                    .and_then(Json::as_str)
                    .map(String::from),
                error: ev
                    .payload
                    .get("status")
                    .and_then(Json::as_str)
                    .filter(|s| *s == "error")
                    .map(String::from),
                turn_without_act: false,
            },
            "action.effect.unknown" | "action.effect.refused" | "model.call.failed" => {
                Observation {
                    seq: ev.seq,
                    event_id: ev.event_id.clone(),
                    key: None,
                    version_id: None,
                    error: Some(ev.class.clone()),
                    turn_without_act: false,
                }
            }
            "control.decision" => {
                if ev
                    .payload
                    .get("kind")
                    .and_then(Json::as_str)
                    .map(|k| k == "act")
                    .unwrap_or(false)
                {
                    last_was_act = true;
                }
                continue;
            }
            "model.call.completed" => {
                let o = Observation {
                    seq: ev.seq,
                    event_id: ev.event_id.clone(),
                    key: None,
                    version_id: None,
                    error: None,
                    turn_without_act: !last_was_act,
                };
                last_was_act = false;
                o
            }
            _ => continue,
        };
        obs.push(o);
    }
    if obs.len() > n {
        obs.split_off(obs.len() - n)
    } else {
        obs
    }
}

/// `exact_repeat` — a `loop_key` cycle of `cycle_len ≤ cycle_max` repeated
/// `threshold` times in the window.
fn exact_repeat(window: &[Observation], policy: &LoopPolicy) -> Option<LoopPattern> {
    let keys: Vec<&str> = window.iter().filter_map(|o| o.key.as_deref()).collect();
    for k in 1..=policy.exact_repeat.cycle_max as usize {
        let need = k * policy.exact_repeat.threshold as usize;
        if keys.len() < need {
            continue;
        }
        let tail = &keys[keys.len() - need..];
        let cycle: Vec<&str> = tail[..k].to_vec();
        let repeats = tail.chunks(k).all(|c| c == cycle.as_slice());
        if repeats {
            return Some(LoopPattern {
                cycle_len: k as u32,
                repeats: policy.exact_repeat.threshold,
                loop_keys: cycle.iter().map(|s| s.to_string()).collect(),
            });
        }
    }
    None
}

/// `no_progress` — `same loop_key ∧ same Observation.version_id` `threshold`
/// times (ADR-0108 D1 — one predicate, one threshold).
fn no_progress(window: &[Observation], policy: &LoopPolicy) -> Option<LoopPattern> {
    let mut seen: Vec<(&str, &str, u32)> = vec![];
    for o in window {
        if let (Some(k), Some(v)) = (o.key.as_deref(), o.version_id.as_deref()) {
            if let Some(e) = seen.iter_mut().find(|(ek, ev_, _)| *ek == k && *ev_ == v) {
                e.2 += 1;
                if e.2 >= policy.no_progress.threshold {
                    return Some(LoopPattern {
                        cycle_len: 1,
                        repeats: e.2,
                        loop_keys: vec![k.to_string()],
                    });
                }
            } else {
                seen.push((k, v, 1));
            }
        }
    }
    None
}

/// `error_streak` — `threshold` consecutive error-class terminals.
fn error_streak(window: &[Observation], policy: &LoopPolicy) -> Option<LoopPattern> {
    let mut streak = 0u32;
    for o in window {
        if o.error.is_some() {
            streak += 1;
        } else if o.key.is_some() || o.turn_without_act {
            streak = 0;
        }
    }
    if streak >= policy.error_streak.threshold {
        Some(LoopPattern {
            cycle_len: 1,
            repeats: streak,
            loop_keys: window
                .iter()
                .rev()
                .filter(|o| o.error.is_some())
                .take(streak as usize)
                .map(|o| o.event_id.clone())
                .collect(),
        })
    } else {
        None
    }
}

/// `monologue` — `threshold` consecutive model turns with no `act` decision.
fn monologue(
    window: &[Observation],
    _events: &[EventEnvelope],
    policy: &LoopPolicy,
) -> Option<LoopPattern> {
    let mut streak = 0u32;
    for o in window {
        if o.turn_without_act {
            streak += 1;
        } else if o.key.is_some() {
            // A proposed/acted call ends the monologue.
            streak = 0;
        }
    }
    if streak >= policy.monologue.threshold {
        Some(LoopPattern {
            cycle_len: 1,
            repeats: streak,
            loop_keys: window
                .iter()
                .rev()
                .filter(|o| o.turn_without_act)
                .take(streak as usize)
                .map(|o| o.event_id.clone())
                .collect(),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_ledger::classes::Durability;
    use hh_ledger::event::{EventPlane, Producer, Scope};
    use hh_ledger::manifest::{ObservabilityLevel, ParticipantClass};

    fn ev(seq: u64, class: &str, payload: Json) -> EventEnvelope {
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
                effect_id: None,
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
            payload,
            prev_hash: "h".into(),
            hash: format!("h{seq}"),
        }
    }

    #[test]
    fn exact_repeat_fires_on_a_repeating_cycle() {
        let p = LoopPolicy::default();
        let mut events = vec![];
        for i in 0..(p.exact_repeat.cycle_max as u64 * p.exact_repeat.threshold as u64) {
            let k = format!("k{}", i % 2);
            events.push(ev(
                i,
                "action.tool.proposed",
                Json::obj([("loop_key", Json::str(&k))]),
            ));
        }
        let hits = detect(&events, &p);
        assert!(hits
            .iter()
            .any(|h| h.detector == LoopDetectorKind::ExactRepeat));
        let hit = hits
            .iter()
            .find(|h| h.detector == LoopDetectorKind::ExactRepeat)
            .unwrap();
        assert_eq!(hit.pattern.cycle_len, 2);
        assert_eq!(hit.action, LadderAction::Nudge); // first rung
    }

    #[test]
    fn the_ladder_walks_nudge_deny_stop_once_each() {
        let p = LoopPolicy::default();
        // One prior detection → next rung is `deny`.
        let events = vec![ev(
            0,
            "control.loop.detected",
            Json::obj([("detector", Json::str("exact_repeat"))]),
        )];
        let st = fold(&events);
        assert_eq!(st.ladder_position, 1);
        assert_eq!(next_rung(&p, &st), Some(LadderAction::Deny));
        // Three → exhausted.
        let events3: Vec<_> = (0..3)
            .map(|i| {
                ev(
                    i,
                    "control.loop.detected",
                    Json::obj([("detector", Json::str("error_streak"))]),
                )
            })
            .collect();
        assert_eq!(next_rung(&p, &fold(&events3)), None);
    }

    #[test]
    fn no_progress_needs_same_key_and_same_version() {
        let p = LoopPolicy::default();
        let mut events = vec![];
        for i in 0..3u64 {
            events.push(ev(
                i,
                "action.effect.observed",
                Json::obj([
                    ("loop_key", Json::str("k")),
                    ("version_id", Json::str("v1")),
                    ("status", Json::str("ok")),
                ]),
            ));
        }
        let hits = detect(&events, &p);
        assert!(hits
            .iter()
            .any(|h| h.detector == LoopDetectorKind::NoProgress));
        // A different version_id must not fire.
        let mut ev2 = vec![];
        for (i, v) in ["v1", "v2", "v3"].iter().enumerate() {
            ev2.push(ev(
                i as u64,
                "action.effect.observed",
                Json::obj([
                    ("loop_key", Json::str("k")),
                    ("version_id", Json::str(*v)),
                    ("status", Json::str("ok")),
                ]),
            ));
        }
        assert!(!detect(&ev2, &p)
            .iter()
            .any(|h| h.detector == LoopDetectorKind::NoProgress));
    }

    #[test]
    fn error_streak_fires_on_consecutive_error_terminals() {
        let p = LoopPolicy::default();
        let events: Vec<_> = (0..3)
            .map(|i| {
                ev(
                    i,
                    "model.call.failed",
                    Json::obj([("error", Json::obj([("class", Json::str("network"))]))]),
                )
            })
            .collect();
        let hits = detect(&events, &p);
        assert!(hits
            .iter()
            .any(|h| h.detector == LoopDetectorKind::ErrorStreak));
    }

    #[test]
    fn a_fresh_streak_after_a_detection_consumes_the_next_rung() {
        // AC-R-2.6.2-1's ladder is persistence-driven: a fired detector's
        // contributing streak resets, but failures *after* the detection
        // are a fresh streak — the detector re-fires and the hit takes
        // the next rung (`nudge` consumed at seq 0 → `deny`).
        let p = LoopPolicy::default();
        let mut events = vec![ev(
            0,
            "control.loop.detected",
            Json::obj([("detector", Json::str("error_streak"))]),
        )];
        for i in 1..4 {
            events.push(ev(i, "model.call.failed", Json::Null));
        }
        let hits = detect(&events, &p);
        let hit = hits
            .iter()
            .find(|h| h.detector == LoopDetectorKind::ErrorStreak)
            .expect("a fresh streak re-fires");
        assert_eq!(hit.action, LadderAction::Deny);
        // …but the *same* evidence never double-fires: a detection row at
        // the tail resets the streak, so a second evaluation sees none.
        events.push(ev(
            4,
            "control.loop.detected",
            Json::obj([("detector", Json::str("error_streak"))]),
        ));
        assert!(!detect(&events, &p)
            .iter()
            .any(|h| h.detector == LoopDetectorKind::ErrorStreak));
    }

    #[test]
    fn judged_and_content_chant_are_not_stage1() {
        for k in LoopDetectorKind::ALL {
            assert_eq!(
                k.admitted_at_stage1(),
                matches!(
                    k,
                    LoopDetectorKind::ExactRepeat
                        | LoopDetectorKind::NoProgress
                        | LoopDetectorKind::ErrorStreak
                        | LoopDetectorKind::Monologue
                )
            );
        }
    }
}
