//! The attempt driver (§5b.1 §2 `attempts`; ADR-0119 d.4 — R-RT-1…6).
//!
//! R-RT-1 the gateway retries only `retry_class ∈ retry_on ⊆ {transient,
//! rate_limit}` — never `permanent`, never `ambiguous`. R-RT-2 a logical call
//! is one `model_call_id` whose `model.call.attempt.*` events share the scope
//! and whose request bytes are **identical** across attempts (a retry never
//! mutates the plan). R-RT-3 `empty_response`/`thinking_only` are classified
//! and retried like transport — the content-invalid classes are `ambiguous`
//! and never retried. R-RT-4 a server `retry-after` above `retry_after_cap_ms`
//! is reported (`will_retry = false`) and the call fails `rate_limited`.
//! R-RT-5 `pause_turn`/`deferred` are never gateway retries. R-RT-6 the
//! policy is data (`AttemptPolicy` from the control envelope).
//!
//! The driver is generic over the attempt body — transport is a port; the
//! sleeper is a port (deterministic replay — no wall clock in the decision).

use crate::events;
use crate::vocab::{AttemptPolicy, ModelError, RetryClass};
use hh_wire::json::Json;

/// The event sink the driver emits `model.call.attempt.*` payloads through —
/// the kernel appends them (the gateway is the sole emitter; the store is the
/// kernel's writer).
pub trait EventSink {
    /// Emit one payload under `class` — `Err` aborts the driver (durable-
    /// before-visible; a dropped audit row is a failure, never silent).
    fn emit(&mut self, class: &str, payload: Json) -> Result<(), String>;
}

/// A sink that drops nothing — collects payloads for the in-process C0 path
/// and tests.
#[derive(Debug, Default)]
pub struct CollectSink {
    /// `(class, payload)` pairs in emission order.
    pub events: Vec<(String, Json)>,
}

impl EventSink for CollectSink {
    fn emit(&mut self, class: &str, payload: Json) -> Result<(), String> {
        self.events.push((class.to_string(), payload));
        Ok(())
    }
}

/// A shared handle over a `RefCell<CollectSink>` — the attempt loop's sink and
/// the attempt body's sink are the same cell (interleaved events keep their
/// order; the caller drains once).
pub struct SharedSink<'s> {
    /// The shared cell.
    pub inner: &'s std::cell::RefCell<CollectSink>,
}

impl EventSink for SharedSink<'_> {
    fn emit(&mut self, class: &str, payload: Json) -> Result<(), String> {
        self.inner.borrow_mut().emit(class, payload)
    }
}

/// The sleeper port — the driver computes `next_delay_ms`; the kernel's
/// `Clock`/`Schedule` port sleeps. `NoSleep` is the deterministic test/replay
/// arm (delays are still *recorded*).
pub trait Sleeper {
    /// Sleep `ms` (the fake records it).
    fn sleep(&mut self, ms: u64);
}

/// A recording sleeper (deterministic — the delay is a fact, not a wait).
#[derive(Debug, Default)]
pub struct NoSleep {
    /// The recorded delays.
    pub slept_ms: Vec<u64>,
}

impl Sleeper for NoSleep {
    fn sleep(&mut self, ms: u64) {
        self.slept_ms.push(ms);
    }
}

/// The outcome of `run_attempts` — the attempt history is a ledger fact.
#[derive(Debug)]
pub struct AttemptOutcome<T> {
    /// The terminal result.
    pub result: Result<T, ModelError>,
    /// How many attempts ran.
    pub attempts: u32,
    /// The recorded delays.
    pub delays_ms: Vec<u64>,
}

/// `run_attempts(policy, model_call_id, now, sink, sleeper, attempt)` — the
/// retry loop. `attempt(attempt_no) -> Result<T, ModelError>` runs **the same
/// bytes** (R-RT-2 — the plan is identical across attempts; the caller binds
/// it once). `now` is the clock port — the span's `duration_ms` is measured,
/// never a zero placeholder.
///
/// `will_retry` is the emitted fact: `retry_class ∈ retry_on` and
/// `attempt_no < max_attempts`; `next_delay_ms` is `honour_retry_after` ?
/// `min(retry_after, cap)` : jittered backoff — deterministic under
/// `(model_call_id, attempt_no)` (the jitter is derived, no RNG).
/// `attempt.started` carries `queue_wait_ms` — the delay the attempt slept
/// before dispatching (R-RT-6).
pub fn run_attempts<T>(
    policy: &AttemptPolicy,
    model_call_id: &str,
    now: &dyn Fn() -> u64,
    sink: &mut dyn EventSink,
    sleeper: &mut dyn Sleeper,
    mut attempt: impl FnMut(u32) -> Result<T, ModelError>,
) -> Result<AttemptOutcome<T>, String> {
    let mut delays = Vec::new();
    let mut waited_ms = 0u64;
    for attempt_no in 1..=policy.max_attempts {
        sink.emit(
            "model.call.attempt.started",
            events::attempt_started(model_call_id, attempt_no, waited_ms),
        )?;
        let started_ms = now();
        match attempt(attempt_no) {
            Ok(v) => {
                sink.emit(
                    "model.call.attempt.completed",
                    events::attempt_completed(
                        model_call_id,
                        attempt_no,
                        now().saturating_sub(started_ms),
                        None,
                    ),
                )?;
                return Ok(AttemptOutcome {
                    result: Ok(v),
                    attempts: attempt_no,
                    delays_ms: delays,
                });
            }
            Err(err) => {
                // R-RT-4 — a `rate_limited` with a server hint above the cap is
                // reported `will_retry = false` and fails `rate_limited`.
                let capped = err.class == crate::vocab::ModelErrorClass::RateLimited
                    && err
                        .retry_after_ms
                        .map(|ms| ms > policy.retry_after_cap_ms)
                        .unwrap_or(false);
                let admitted = policy.admits_retry(err.retry_class)
                    && attempt_no < policy.max_attempts
                    && !capped;
                let next_delay = if admitted {
                    let backoff = policy.backoff_ms(model_call_id, attempt_no);
                    let delay = if policy.honour_retry_after {
                        err.retry_after_ms
                            .map(|hint| hint.min(policy.retry_after_cap_ms).max(backoff))
                            .unwrap_or(backoff)
                    } else {
                        backoff
                    };
                    Some(delay)
                } else {
                    None
                };
                sink.emit(
                    "model.call.attempt.failed",
                    events::attempt_failed(model_call_id, attempt_no, &err, admitted, next_delay),
                )?;
                if admitted {
                    let d = next_delay.unwrap_or(0);
                    delays.push(d);
                    sleeper.sleep(d);
                    waited_ms = d;
                    continue;
                }
                return Ok(AttemptOutcome {
                    result: Err(err),
                    attempts: attempt_no,
                    delays_ms: delays,
                });
            }
        }
    }
    // Unreachable — `max_attempts` bounds the loop and every iteration returns.
    Ok(AttemptOutcome {
        result: Err(ModelError::new(
            crate::vocab::ModelErrorClass::Unknown,
            "attempt loop exhausted without a terminal",
        )),
        attempts: policy.max_attempts,
        delays_ms: delays,
    })
}

/// Whether an error is retryable under the policy (the caller's read of
/// R-RT-1 — `pause_turn`/`deferred` never reach here: they are `StopReason`s,
/// not `ModelError`s, and the driver never sees them).
pub fn retryable(policy: &AttemptPolicy, retry_class: RetryClass) -> bool {
    policy.admits_retry(retry_class)
}
