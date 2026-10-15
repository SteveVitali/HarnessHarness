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
    /// `judged` metadata — the `validator_ref` and observed confidence
    /// (ppm) the `control.loop.detected` row names (`None` for the
    /// deterministic arms).
    pub validator_ref: Option<String>,
    /// The judge's observed confidence (ppm) — `judged` hits only.
    pub confidence_ppm: Option<u64>,
    /// `true` when a `judged` hit's rung was clamped `stop → deny`
    /// (ADR-0108's rule: a judged detector is never sole grounds for a
    /// stop — the clamp is what the row records).
    pub stop_clamped: bool,
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
                validator_ref: None,
                confidence_ppm: None,
                stop_clamped: false,
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
#[allow(clippy::items_after_test_module)] // test module precedes the S4.16b judged port
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

    // ── the `judged` arm (§5e.2 C1; ADR-0108; AC-R-2.6.2-11) ────────────

    #[derive(Debug)]
    struct AlwaysJudge(u64);
    impl JudgePort for AlwaysJudge {
        fn judge(&self, _spec: &crate::policy::JudgedSpec, _w: &[Json]) -> u64 {
            self.0
        }
    }

    fn judged_policy(after_turns: u32, interval: u32, threshold_ppm: u64) -> LoopPolicy {
        LoopPolicy {
            judged: Some(crate::policy::JudgedSpec {
                validator_ref: "validator:judge-v1".into(),
                after_turns,
                interval,
                confidence_threshold_ppm: threshold_ppm,
                charged_to: "budget:root".into(),
            }),
            ..LoopPolicy::default()
        }
    }

    #[test]
    fn judged_hit_at_cadence_with_confidence() {
        let p = judged_policy(2, 1, 500_000);
        // Two model turns — the cadence admits the judge at `after_turns`.
        let events = vec![
            ev(0, "model.call.completed", Json::Null),
            ev(1, "model.call.completed", Json::Null),
        ];
        let hits = detect_with_judge(&events, &p, Some(&AlwaysJudge(900_000)));
        let hit = hits
            .iter()
            .find(|h| h.detector == LoopDetectorKind::Judged)
            .expect("a confident judge at cadence is a hit");
        assert_eq!(hit.validator_ref.as_deref(), Some("validator:judge-v1"));
        assert_eq!(hit.confidence_ppm, Some(900_000));
        assert_eq!(hit.action, LadderAction::Nudge); // first rung
    }

    #[test]
    fn judged_below_threshold_and_no_port_emit_nothing() {
        let p = judged_policy(1, 1, 500_000);
        let events = vec![ev(0, "model.call.completed", Json::Null)];
        // Confidence below the floor — no hit.
        assert!(detect_with_judge(&events, &p, Some(&AlwaysJudge(100_000)))
            .iter()
            .all(|h| h.detector != LoopDetectorKind::Judged));
        // A declared spec with no bound port emits nothing (a missing
        // validator is silent-vs-fabricated, never a guess).
        assert!(detect_with_judge(&events, &p, None)
            .iter()
            .all(|h| h.detector != LoopDetectorKind::Judged));
    }

    // ── R2.15 — the recorded judge (offline `Validator{kind = judge}`
    // binding over recorded `model_io`; DF-S1.21-3) ──────────────────

    /// A recorded `judged` verdict replays as the port's answer — the
    /// judge call ran over recorded model_io, never a fabricated live
    /// judgment; an unbound `validator_ref` abstains (no hit, identical
    /// to the `judge: None` arm).
    #[test]
    fn recorded_judge_replays_and_abstains() {
        let p = judged_policy(1, 1, 500_000);
        let verdict = Json::obj([
            ("validator_ref", Json::str("validator:judge-v1")),
            ("detector", Json::str("judged")),
            ("value", Json::Int(900_000)),
        ]);
        let mut events = vec![ev(0, "model.call.completed", Json::Null)];
        // A recorded judged verdict row on the prefix (the judge call's
        // model_io record) — `verification.validator.verdict` at
        // `detector = judged`.
        let v = ev(1, "verification.validator.verdict", verdict);
        events.push(v.clone());
        let judge = RecordedJudge::from_prefix(&events);
        assert!(judge.has_recording("validator:judge-v1"));
        let hits = detect_with_judge(&events, &p, Some(&judge));
        let hit = hits
            .iter()
            .find(|h| h.detector == LoopDetectorKind::Judged)
            .expect("a recorded confident judgment replays as a hit");
        assert_eq!(hit.confidence_ppm, Some(900_000));

        // A deterministic verdict is never replayed as a judged answer.
        let det = Json::obj([
            ("validator_ref", Json::str("validator:judge-v1")),
            ("detector", Json::str("deterministic")),
            ("value", Json::Int(900_000)),
        ]);
        let judge = RecordedJudge::from_prefix(&[ev(1, "verification.validator.verdict", det)]);
        assert!(!judge.has_recording("validator:judge-v1"));
        assert!(detect_with_judge(&events, &p, Some(&judge))
            .iter()
            .all(|h| h.detector != LoopDetectorKind::Judged));

        // A bound judge naming a ref with no recording abstains.
        let judge = RecordedJudge::default();
        assert!(detect_with_judge(&events, &p, Some(&judge))
            .iter()
            .all(|h| h.detector != LoopDetectorKind::Judged));
    }

    #[test]
    fn judged_stop_rung_clamps_to_deny() {
        let p = judged_policy(1, 1, 500_000);
        // Two prior detections — the next rung would be `stop`; a judged
        // hit is never sole grounds for a stop (ADR-0108 clamp).
        let events = vec![
            ev(
                0,
                "control.loop.detected",
                Json::obj([("detector", Json::str("judged"))]),
            ),
            ev(
                1,
                "control.loop.detected",
                Json::obj([("detector", Json::str("judged"))]),
            ),
            ev(2, "model.call.completed", Json::Null),
        ];
        let hits = detect_with_judge(&events, &p, Some(&AlwaysJudge(1_000_000)));
        let hit = hits
            .iter()
            .find(|h| h.detector == LoopDetectorKind::Judged)
            .expect("clamped hit still lands");
        assert_eq!(hit.action, LadderAction::Deny);
        assert!(hit.stop_clamped, "the row records the stop → deny clamp");
    }

    #[test]
    fn judged_cadence_does_not_refire_within_interval() {
        let p = judged_policy(1, 2, 500_000);
        // A prior judged hit at turn 1; interval=2 — turn 2 must not
        // re-fire.
        let events = vec![
            ev(0, "model.call.completed", Json::Null),
            ev(
                1,
                "control.loop.detected",
                Json::obj([("detector", Json::str("judged"))]),
            ),
            ev(2, "model.call.completed", Json::Null),
        ];
        assert!(
            detect_with_judge(&events, &p, Some(&AlwaysJudge(1_000_000)))
                .iter()
                .all(|h| h.detector != LoopDetectorKind::Judged)
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// `judged` — the C1 detector arm (§5e.2; ADR-0108 D1; AC-R-2.6.2-11)
// ─────────────────────────────────────────────────────────────────────────────

/// The `Validator{kind: judge}` invocation port — the driver binds an
/// accountable validator; the port answers the detector's confidence
/// (ppm ∈ 0..=1_000_000) for the current observation window. A `judged`
/// hit is *detected* at cadence and may walk the `nudge → deny` rungs —
/// the `stop` rung is never a judged-only act (the clamp below).
pub trait JudgePort: std::fmt::Debug {
    /// `judge(spec, window) → confidence_ppm` — `window` carries the
    /// trailing observation summaries the deterministic detectors see
    /// (`{seq, event_id, key, version_id, error, turn_without_act}` —
    /// data, never surface text).
    fn judge(&self, spec: &crate::policy::JudgedSpec, window: &[Json]) -> u64;
}

/// `RecordedJudge` — the offline `Validator{kind = judge}` binding (R2.15;
/// DF-S1.21-3's honest half): a `JudgePort` whose answers *replay the
/// recorded `model_io`* — the durable `verification.validator.verdict` /
/// `verification.critic.verdict` rows at `detector = judged` the judge's
/// prior call ledgered — never a fabricated live-model judgment
/// (`offline-only` is the honesty ceiling). A `validator_ref` with no
/// recorded judged verdict **abstains**: the port answers 0, which — like
/// `judge: None` — mints no hit; an abstention is never rendered as a
/// 0-confidence claim and nothing is emitted on its account.
#[derive(Debug, Default)]
pub struct RecordedJudge {
    /// `validator_ref → confidence_ppm` — the latest recorded judged
    /// verdict per bound validator.
    answers: std::collections::BTreeMap<String, u64>,
}

impl RecordedJudge {
    /// `from_prefix(events)` — fold the durable prefix for recorded judged
    /// verdicts: `verification.validator.verdict`/`verification.critic.
    /// verdict` rows carrying `detector = "judged"`. The replayed
    /// confidence is the row's explicit `confidence` member, else a
    /// graded `value` (ppm int); a bool/lattice value records nothing —
    /// the judge's own recorded confidence is the answer, never an
    /// inferred one.
    pub fn from_prefix(events: &[EventEnvelope]) -> RecordedJudge {
        let mut answers = std::collections::BTreeMap::new();
        for e in events {
            if !matches!(
                e.class.as_str(),
                "verification.validator.verdict" | "verification.critic.verdict"
            ) {
                continue;
            }
            if e.payload.get("detector").and_then(Json::as_str) != Some("judged") {
                continue;
            }
            let Some(vr) = e
                .payload
                .get("validator_ref")
                .or_else(|| e.payload.get("critic_ref"))
                .and_then(Json::as_str)
            else {
                continue;
            };
            let confidence = e
                .payload
                .get("confidence")
                .and_then(Json::as_int)
                .or_else(|| e.payload.get("value").and_then(Json::as_int));
            if let Some(c) = confidence {
                answers.insert(vr.to_string(), c.max(0) as u64);
            }
        }
        RecordedJudge { answers }
    }

    /// Whether a recorded judged verdict exists for `validator_ref` —
    /// the binding surface (`spec.validator_ref`) names the judge the
    /// replay answers for; an unbound ref abstains.
    pub fn has_recording(&self, validator_ref: &str) -> bool {
        self.answers.contains_key(validator_ref)
    }
}

impl JudgePort for RecordedJudge {
    fn judge(&self, spec: &crate::policy::JudgedSpec, _window: &[Json]) -> u64 {
        // A recorded answer replays its recorded confidence; no recording
        // ⇒ abstain (0 ⇒ below any admissible threshold ⇒ no hit —
        // identical to the `judge: None` silent arm, never a guess).
        self.answers.get(&spec.validator_ref).copied().unwrap_or(0)
    }
}

/// `detect_with_judge(events, policy, judge)` — the Stage-4 admission of
/// `LoopPolicy.judged`: the four deterministic detectors run as in
/// [`detect`], then — at `after_turns`/`interval` cadence — the bound
/// judge port answers; a confidence ≥ `confidence_threshold_ppm` is a
/// `control.loop.detected{detector: judged, validator_ref}` hit on the
/// same global ladder, with `stop` clamped to `deny` (a judged detector
/// is never sole grounds for stopping — deterministic arms still own
/// `stop`). `judge: None` with a declared spec emits no judged hit (a
/// declared detector without its validator is silent, never guessed).
pub fn detect_with_judge(
    events: &[EventEnvelope],
    policy: &LoopPolicy,
    judge: Option<&dyn JudgePort>,
) -> Vec<LoopHit> {
    let mut hits = detect(events, policy);
    let Some(spec) = &policy.judged else {
        return hits;
    };
    let Some(judge) = judge else {
        return hits;
    };
    // Cadence — turns = `model.call.completed` rows so far; the judge
    // may fire at `after_turns` and every `interval` turns after, but
    // not twice at the same cadence point (the last judged hit's turn
    // position gates re-fire).
    let turns = events
        .iter()
        .filter(|e| e.class == "model.call.completed")
        .count() as u32;
    if turns < spec.after_turns || !(turns - spec.after_turns).is_multiple_of(spec.interval) {
        return hits;
    }
    let last_judged_pos = events
        .iter()
        .filter(|e| {
            e.class == "control.loop.detected"
                && e.payload.get("detector").and_then(Json::as_str) == Some("judged")
        })
        .map(|e| {
            events
                .iter()
                .take_while(|x| x.seq <= e.seq)
                .filter(|x| x.class == "model.call.completed")
                .count() as u32
        })
        .max();
    if last_judged_pos.is_some_and(|t| turns < t + spec.interval) {
        return hits;
    }
    let window = collect_window(events, policy.window_events as usize);
    let window_json: Vec<Json> = window
        .iter()
        .map(|o| {
            Json::obj([
                ("seq", Json::Int(o.seq as i64)),
                ("event_id", Json::str(o.event_id.clone())),
                (
                    "key",
                    o.key
                        .as_ref()
                        .map(|k| Json::str(k.clone()))
                        .unwrap_or(Json::Null),
                ),
                (
                    "version_id",
                    o.version_id
                        .as_ref()
                        .map(|v| Json::str(v.clone()))
                        .unwrap_or(Json::Null),
                ),
                (
                    "error",
                    o.error
                        .as_ref()
                        .map(|e| Json::str(e.clone()))
                        .unwrap_or(Json::Null),
                ),
                ("turn_without_act", Json::Bool(o.turn_without_act)),
            ])
        })
        .collect();
    let confidence = judge.judge(spec, &window_json);
    if confidence < spec.confidence_threshold_ppm {
        return hits;
    }
    // The shared ladder — the judged hit occupies the same `nudge → deny
    // → stop` sequence the deterministic arms do (one rung per hit).
    let state = fold(events);
    let Some(rung) = next_rung(policy, &state) else {
        return hits;
    };
    let (action, clamped) = if rung == LadderAction::Stop {
        // Never sole grounds: the clamp is recorded, the run ends only
        // on deterministic evidence or an orthogonal stop rule.
        (LadderAction::Deny, true)
    } else {
        (rung, false)
    };
    let evidence_refs: Vec<String> = window.iter().map(|o| o.event_id.clone()).collect();
    hits.push(LoopHit {
        detector: LoopDetectorKind::Judged,
        pattern: LoopPattern {
            cycle_len: 0,
            repeats: (confidence / 10_000).min(u32::MAX as u64) as u32, // confidence %
            loop_keys: window.iter().filter_map(|o| o.key.clone()).collect(),
        },
        action,
        ladder_position: state.ladder_position,
        evidence_refs,
        validator_ref: Some(spec.validator_ref.clone()),
        confidence_ppm: Some(confidence),
        stop_clamped: clamped,
    });
    hits
}
