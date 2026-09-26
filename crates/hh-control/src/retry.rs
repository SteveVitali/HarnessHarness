//! `RetryPolicy`/`TimeoutPolicy` mechanics (§5e.2; ADR-0107 D1–D7): shared
//! attempt identity (INV-5 — `attempt_no` strictly increases per scope id,
//! derived by folding the durable prefix, never from process memory), the
//! single `retries` counter (INV-6), `attempt_delta` recording, derived
//! scope deadlines (`deadline(scope) = min(TimeoutPolicy[kind].hard_max,
//! remaining of every `time.*` hard ceiling on the ancestor chain)` — an
//! effect's deadline additionally capped by `time.working_ms` remaining,
//! OQ-119), `extend_on_progress` bounded by `hard_max`, `expire` writing the
//! kind-fixed `ScopeTerminal` + `control.timeout.fired`, and the ADR-0107 D6
//! reservation sizing rule.
//!
//! Kernel constraints no definition may relax live here as constants of the
//! fold: `context_length_exceeded` is never retried with the same request;
//! `invalid_request`/`content_policy`/`cancelled` are non-retryable;
//! `irreversible` effects never auto-redispatch (INV-8 — enforced at
//! G-PRE-DISPATCH via [`redispatch_permitted`]).

use hh_ledger::event::EventEnvelope;
use hh_wire::json::Json;

use crate::policy::{MaxAttempts, RetryPolicy, ScopeTerminal, TimeoutPolicy};
use crate::vocab::ScopeKind;

/// The kernel-fixed non-retryable classes (§5e.2 kernel constraints —
/// `context_length_exceeded` is a compaction trigger, `invalid_request`/
/// `content_policy`/`cancelled` are never retried — a `RetrySpec` for one of
/// these is rejected at `validate`; the fold also refuses it so a hand-made
/// policy cannot widen, INV-7).
pub const NEVER_RETRYABLE: &[&str] = &[
    "context_length_exceeded",
    "invalid_request",
    "content_policy",
    "cancelled",
    "containment_denied",
];

/// `attempt_no(scope_id)` — the shared-identity fold: the greatest
/// `attempt_no` any durable row recorded for the scope (model calls carry it
/// on `model.call.requested`; effects on `action.effect.committed`;
/// `control.retry.scheduled` rows carry the *next* attempt — the fold takes
/// the max across all three so a crash mid-schedule never reissues an
/// attempt number, INV-5).
pub fn attempt_no(events: &[EventEnvelope], scope_id: &str) -> u64 {
    let mut n = 0u64;
    for ev in events {
        let p = &ev.payload;
        let is_scope = [
            ev.scope.model_call_id.as_deref(),
            ev.scope.effect_id.as_deref(),
            ev.scope.tool_call_id.as_deref(),
            ev.scope.child_run_id.as_deref(),
        ]
        .iter()
        .flatten()
        .any(|id| *id == scope_id)
            || p.get("scope_id").and_then(Json::as_str) == Some(scope_id);
        if !is_scope {
            continue;
        }
        for member in ["attempt_no", "attempt"] {
            if let Some(a) = p.get(member).and_then(Json::as_int) {
                if a > 0 {
                    n = n.max(a as u64);
                }
            }
        }
    }
    n
}

/// `give_up` — the scope is terminal with `error_class`; G-POST-EFFECT
/// decides (ADR-0107 D3).
#[derive(Debug, Clone, PartialEq)]
pub struct GiveUp {
    /// The error class the scope terminates with.
    pub error_class: String,
}

/// `retry` — the scheduled re-drive (`control.retry.scheduled` IS the timer).
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledRetry {
    /// The next attempt (strictly greater than every prior — INV-5).
    pub attempt_no: u64,
    /// `not_before` (wall-ms) — `retry-after` may only extend it.
    pub not_before: u64,
    /// The computed delay.
    pub delay_ms: u64,
    /// The recorded `attempt_delta`, if the retry changes the request.
    pub attempt_delta: Option<crate::vocab::DeltaKind>,
}

/// `schedule_retry`'s product.
#[derive(Debug, Clone, PartialEq)]
pub enum RetryOutcome {
    /// `retry{attempt_no, not_before, attempt_delta?}`.
    Retry(ScheduledRetry),
    /// `give_up{error_class}` — class bound hit or non-retryable class.
    GiveUp(GiveUp),
    /// `RetryBudgetExhausted` — the single `retries` counter has no headroom
    /// (INV-6 — even an `unbounded` per-class spec dies here).
    BudgetExhausted {
        /// The error class (the scope still terminates with it).
        error_class: String,
    },
}

/// `schedule_retry(run, lease, scope_id, error)` — pure over the durable
/// prefix + policy: the attempt is `attempt_no(scope) + 1`, the delay is the
/// spec's deterministic `delay_ms(attempt, jitter_key = scope_id)` —
/// `honour_retry_after` only ever extends `not_before` (`now_ms` is the
/// caller's logical clock — injectable, never `Instant::now`).
#[allow(clippy::too_many_arguments)] // the F2 seam's record arity
pub fn schedule_retry(
    events: &[EventEnvelope],
    policy: &RetryPolicy,
    kind: ScopeKind,
    scope_id: &str,
    error_class: &str,
    now_ms: u64,
    retry_after_ms: Option<u64>,
    attempt_delta: Option<crate::vocab::DeltaKind>,
    retries_used: u64,
    retries_ceiling: u64,
) -> RetryOutcome {
    // Kernel constraint — a never-retryable class gives up regardless of
    // the table (a definition cannot relax it).
    if NEVER_RETRYABLE.contains(&error_class) {
        return RetryOutcome::GiveUp(GiveUp {
            error_class: error_class.into(),
        });
    }
    let spec = match policy.spec(kind, error_class) {
        Some(s) => s,
        None => {
            return RetryOutcome::GiveUp(GiveUp {
                error_class: error_class.into(),
            })
        }
    };
    let attempt = attempt_no(events, scope_id) + 1;
    if let MaxAttempts::Bounded(max) = spec.max_attempts {
        if attempt > max as u64 {
            return RetryOutcome::GiveUp(GiveUp {
                error_class: error_class.into(),
            });
        }
    }
    // INV-6 — the single `retries` counter bounds every feedback path.
    if retries_used >= retries_ceiling {
        return RetryOutcome::BudgetExhausted {
            error_class: error_class.into(),
        };
    }
    // `attempt_delta` — a retry that changes the request records it; a
    // delta outside `attempt_delta_allowed` is a give_up (never silent).
    if let Some(d) = &attempt_delta {
        let allowed = spec
            .attempt_delta_allowed
            .iter()
            .any(|a| a.tag() == d.tag());
        if !allowed {
            return RetryOutcome::GiveUp(GiveUp {
                error_class: error_class.into(),
            });
        }
    }
    let mut not_before = now_ms.saturating_add(spec.delay_ms(attempt, scope_id));
    if spec.honour_retry_after {
        if let Some(ra) = retry_after_ms {
            // `retry-after` extends, never shortens.
            not_before = not_before.max(now_ms.saturating_add(ra));
        }
    }
    RetryOutcome::Retry(ScheduledRetry {
        attempt_no: attempt,
        not_before,
        delay_ms: not_before.saturating_sub(now_ms),
        attempt_delta,
    })
}

/// `Deadline{at, hard_max_at, extend_on_progress}` (§5e.2 `deadline()` row).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline {
    /// The effective deadline (ms since the scope's `started_at`).
    pub at: u64,
    /// The absolute cap (`extend_on_progress` never crosses it).
    pub hard_max_at: u64,
    /// Whether progress extends `at` (bounded by `hard_max_at`).
    pub extend_on_progress: bool,
}

/// `deadline(scope_kind, started_at, ancestors)` — `min(TimeoutPolicy[kind]
/// .hard_max, remaining of every `time.*` hard ceiling on the ancestor
/// chain)`; `ancestor_caps_ms` are the remaining `time.*` ceilings the
/// caller folded (a child's deadline never exceeds its ancestors');
/// `working_ms_cap` is the `time.working_ms` remainder an *effect* deadline
/// additionally respects (OQ-119). `default_ms = 0` (an `attended`
/// permission wait) yields `at = u64::MAX` — no deadline.
pub fn deadline(
    policy: &TimeoutPolicy,
    kind: ScopeKind,
    started_at: u64,
    ancestor_caps_ms: &[u64],
    working_ms_cap: Option<u64>,
) -> Deadline {
    let spec = policy.spec(kind);
    let default_ms = spec.map(|s| s.default_ms).unwrap_or(0);
    let hard_max = spec.map(|s| s.hard_max_ms).unwrap_or(u64::MAX);
    let mut hard = started_at.saturating_add(hard_max);
    for c in ancestor_caps_ms {
        hard = hard.min(*c);
    }
    if kind == ScopeKind::ToolAttempt {
        if let Some(w) = working_ms_cap {
            hard = hard.min(w);
        }
    }
    let at = if default_ms == 0 {
        hard // `attended` permission wait — the deadline IS the hard cap.
    } else {
        started_at.saturating_add(default_ms).min(hard)
    };
    Deadline {
        at,
        hard_max_at: hard,
        extend_on_progress: spec.map(|s| s.extend_on_progress).unwrap_or(false),
    }
}

/// `extend(d, progress_at)` — `extend_on_progress` extends `at` by one
/// `default_ms`, never beyond `hard_max_at` (kernel constraint).
pub fn extend_on_progress(
    policy: &TimeoutPolicy,
    kind: ScopeKind,
    d: Deadline,
    progress_at: u64,
) -> Deadline {
    if !d.extend_on_progress {
        return d;
    }
    let bump = policy.spec(kind).map(|s| s.default_ms).unwrap_or(0);
    Deadline {
        at: progress_at
            .saturating_add(bump)
            .min(d.hard_max_at)
            .max(d.at),
        ..d
    }
}

/// `expire`'s kind-fixed terminal (`TimeoutSpec.on_expiry` — `for_read_only`
/// degrades `unknown{timeout}` to `observed(not_applied)` when the effect
/// never wrote ahead).
pub fn expiry_terminal(policy: &TimeoutPolicy, kind: ScopeKind, read_only: bool) -> ScopeTerminal {
    let t = policy
        .spec(kind)
        .map(|s| s.on_expiry)
        .unwrap_or_else(|| ScopeTerminal::for_kind(kind));
    if read_only {
        t.for_read_only()
    } else {
        t
    }
}

/// INV-8 — `redispatch_permitted(effect_class, last_terminal)`: no automatic
/// redispatch of `irreversible`; no redispatch of `unknown` without a probe
/// or an idempotent class.
pub fn redispatch_permitted(
    effect_class: &str,
    last_terminal_unknown: bool,
    probed_or_idempotent: bool,
) -> bool {
    if effect_class == "irreversible" {
        return false;
    }
    if last_terminal_unknown && !probed_or_idempotent {
        return false;
    }
    true
}

/// ADR-0107 D6 reservation sizing — `reserve(model_call, size)`:
/// sequential: `min(profile max_output bound, remaining)`;
/// `pool` children: `min(declared bound, learned_p95?, remaining ÷
/// active_children)` — the p95 term is the caller's (the scheduler's
/// `expected_cost` while a non-`static` `compute_policy` is bound — CF-404;
/// Stage-1 passes `None` ⇒ the declared bound alone).
pub fn reserve_model_call_size(profile_max_output_bound: u64, remaining: u64) -> u64 {
    profile_max_output_bound.min(remaining)
}

/// The `pool` sizing (children divide the remainder).
pub fn reserve_pool_child_size(
    declared_bound: u64,
    learned_p95_or_expected: Option<u64>,
    remaining: u64,
    active_children: u32,
) -> u64 {
    let share = remaining / active_children.max(1) as u64;
    declared_bound
        .min(learned_p95_or_expected.unwrap_or(u64::MAX))
        .min(share)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{Backoff, RetrySpec};
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
                model_call_id: Some("m-1".into()),
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
            hash: "h".into(),
        }
    }

    fn policy() -> RetryPolicy {
        let mut p = RetryPolicy::default();
        p.insert_model_defaults();
        p
    }

    #[test]
    fn attempt_identity_is_shared_and_monotone() {
        let events = vec![
            ev(
                0,
                "model.call.requested",
                Json::obj([("attempt_no", Json::Int(1))]),
            ),
            ev(
                1,
                "model.call.requested",
                Json::obj([("attempt_no", Json::Int(2))]),
            ),
            ev(
                2,
                "control.retry.scheduled",
                Json::obj([("scope_id", Json::str("m-1")), ("attempt_no", Json::Int(3))]),
            ),
        ];
        assert_eq!(attempt_no(&events, "m-1"), 3);
        assert_eq!(attempt_no(&events, "other"), 0);
    }

    #[test]
    fn transient_retries_then_gives_up_at_the_bound() {
        let p = policy();
        // attempt 1 recorded → schedule yields attempt 2.
        let events = vec![ev(
            0,
            "model.call.requested",
            Json::obj([("attempt_no", Json::Int(1))]),
        )];
        let r = schedule_retry(
            &events,
            &p,
            ScopeKind::ModelCall,
            "m-1",
            "network",
            0,
            None,
            None,
            0,
            100,
        );
        match r {
            RetryOutcome::Retry(s) => {
                assert_eq!(s.attempt_no, 2);
                assert!(s.delay_ms >= 250); // initial backoff + jitter ≥ base
            }
            other => panic!("{other:?}"),
        }
        // attempt 3 recorded → next is 4 > Bounded(3) → give_up.
        let events3 = vec![ev(
            0,
            "model.call.requested",
            Json::obj([("attempt_no", Json::Int(3))]),
        )];
        assert!(matches!(
            schedule_retry(
                &events3,
                &p,
                ScopeKind::ModelCall,
                "m-1",
                "network",
                0,
                None,
                None,
                0,
                100
            ),
            RetryOutcome::GiveUp(_)
        ));
    }

    #[test]
    fn never_retryable_classes_give_up_regardless_of_the_table() {
        let mut p = policy();
        // A hand-made spec cannot widen (INV-7).
        p.set(
            ScopeKind::ModelCall,
            "cancelled",
            RetrySpec {
                max_attempts: MaxAttempts::Unbounded,
                backoff: Backoff {
                    initial_ms: 1,
                    factor_ppm: 1_000_000,
                    max_ms: 1,
                    jitter_ppm: 0,
                },
                honour_retry_after: false,
                attempt_delta_allowed: vec![],
            },
        );
        for class in NEVER_RETRYABLE {
            assert!(
                matches!(
                    schedule_retry(
                        &[],
                        &p,
                        ScopeKind::ModelCall,
                        "m",
                        class,
                        0,
                        None,
                        None,
                        0,
                        100
                    ),
                    RetryOutcome::GiveUp(_)
                ),
                "{class} must give up"
            );
        }
    }

    #[test]
    fn the_retries_counter_bounds_unbounded_specs() {
        let mut p = RetryPolicy::default();
        p.set(
            ScopeKind::ModelCall,
            "network",
            RetrySpec {
                max_attempts: MaxAttempts::Unbounded,
                backoff: Backoff {
                    initial_ms: 10,
                    factor_ppm: 1_000_000,
                    max_ms: 10,
                    jitter_ppm: 0,
                },
                honour_retry_after: false,
                attempt_delta_allowed: vec![],
            },
        );
        assert!(matches!(
            schedule_retry(
                &[],
                &p,
                ScopeKind::ModelCall,
                "m",
                "network",
                0,
                None,
                None,
                5,
                5
            ),
            RetryOutcome::BudgetExhausted { .. }
        ));
    }

    #[test]
    fn deterministic_delays_and_retry_after_only_extends() {
        let p = policy();
        let a = schedule_retry(
            &[],
            &p,
            ScopeKind::ModelCall,
            "m-1",
            "network",
            0,
            None,
            None,
            0,
            100,
        );
        let b = schedule_retry(
            &[],
            &p,
            ScopeKind::ModelCall,
            "m-1",
            "network",
            0,
            None,
            None,
            0,
            100,
        );
        assert_eq!(a, b); // deterministic — same inputs, same delay
        let with_ra = schedule_retry(
            &[],
            &p,
            ScopeKind::ModelCall,
            "m-1",
            "rate_limited",
            0,
            Some(60_000),
            None,
            0,
            100,
        );
        match with_ra {
            RetryOutcome::Retry(s) => assert!(s.not_before >= 60_000),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn deadline_is_min_of_hard_max_and_ancestor_caps() {
        let mut tp = TimeoutPolicy::default();
        tp.insert_kind_defaults();
        let d = deadline(&tp, ScopeKind::ModelCall, 1_000, &[50_000], None);
        assert_eq!(d.hard_max_at, 50_000); // ancestor cap binds below hard_max
        assert_eq!(d.at, 50_000);
        // An effect additionally respects time.working_ms.
        let e = deadline(&tp, ScopeKind::ToolAttempt, 0, &[u64::MAX], Some(10_000));
        assert_eq!(e.hard_max_at, 10_000);
    }

    #[test]
    fn extend_on_progress_never_crosses_hard_max() {
        let mut tp = TimeoutPolicy::default();
        tp.insert_kind_defaults();
        let d = deadline(&tp, ScopeKind::ToolAttempt, 0, &[], None);
        let e = extend_on_progress(&tp, ScopeKind::ToolAttempt, d, d.hard_max_at);
        assert_eq!(e.at, d.hard_max_at);
    }

    #[test]
    fn irreversible_never_redispatches_and_unknown_needs_probe() {
        assert!(!redispatch_permitted("irreversible", false, true));
        assert!(!redispatch_permitted("reversible", true, false));
        assert!(redispatch_permitted("reversible", true, true));
        assert!(redispatch_permitted("reversible", false, false));
    }

    #[test]
    fn reservation_sizing() {
        assert_eq!(reserve_model_call_size(1_000, 500), 500);
        assert_eq!(reserve_model_call_size(1_000, 5_000), 1_000);
        assert_eq!(reserve_pool_child_size(1_000, None, 3_000, 4), 750);
        assert_eq!(reserve_pool_child_size(1_000, Some(100), 3_000, 4), 100);
    }
}
