//! The deadline hierarchy + two-phase class-aware interruption (§5d.5 §5;
//! ADR-0102 D5/D6). The ladder is `run_deadline ≥ phase_deadline ≥
//! effect_deadline ≥ attempt_deadline` — a tighter outer bound wins, never
//! the reverse; the *effective* deadline is the minimum. Interruption is
//! two-phase and class-aware: `signalled{term}` (graceful) then
//! `signalled{kill}` — a `non_idempotent`/`irreversible` effect that may have
//! applied goes to `unknown` → probe, never a silent redispatch.

/// `DeadlineLadder` — the four nested bounds (the effective deadline is the
/// tightest).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeadlineLadder {
    /// The run's deadline (mono ms).
    pub run_deadline_ms: Option<u64>,
    /// The phase's deadline.
    pub phase_deadline_ms: Option<u64>,
    /// The effect's deadline.
    pub effect_deadline_ms: Option<u64>,
    /// The attempt's deadline (the tool-call timeout).
    pub attempt_deadline_ms: Option<u64>,
}

impl DeadlineLadder {
    /// The effective attempt deadline — `min` of the set bounds (`None` = no
    /// bound; the tightest wins, never the loosest).
    pub fn effective(&self) -> Option<u64> {
        [
            self.run_deadline_ms,
            self.phase_deadline_ms,
            self.effect_deadline_ms,
            self.attempt_deadline_ms,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// Whether `now` has passed the effective deadline.
    pub fn expired(&self, now: u64) -> bool {
        self.effective().is_some_and(|d| now >= d)
    }
}

/// `InterruptPhase` — the two-phase interruption (`term` → `kill`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptPhase {
    /// Phase 1 — graceful `term` (the tool may clean up; the helper waits
    /// `grace_ms`).
    Term,
    /// Phase 2 — `kill` (after `grace_ms`; the effect is `signalled{kill}`).
    Kill,
}

/// `CancelPolicy` — the interruption contract a `CancelBy` drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancelPolicy {
    /// The grace window between `term` and `kill` (ms).
    pub grace_ms: u64,
}

impl Default for CancelPolicy {
    fn default() -> Self {
        CancelPolicy { grace_ms: 2_000 }
    }
}
