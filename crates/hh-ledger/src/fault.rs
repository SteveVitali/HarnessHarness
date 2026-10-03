//! The kill-point vocabulary (R-2.2.3⁰ᶜ; §5a.3's fault-injection matrix;
//! ADR-0132 §4–5). `KillPoint` names the durable boundary a fault injector
//! stops at — "runtime death *after* the named row is durable, before the
//! next one lands". Every row before the kill point is already durable;
//! nothing after lands. The battery ([`hh_battery`]) arms a point, drives
//! the canonical sequence through the real seams, drops the store mid-run,
//! reopens, and asserts the §5a.3 recovery-table outcome per effect class.
//!
//! Two seams carry the vocabulary:
//!
//! * `Store::inject_durability_faults(n)` — the next `n` `append` calls fail
//!   `Durability{injected}` before a byte is written (KP-9 — the batch is
//!   absent as a unit; atomicity is the append path's own guarantee).
//! * `Store::arm_restore_kill(after)` — `restore` aborts after `after`
//!   recovery appends with `FaultInjected{kp13}` (KP-13 — restore is
//!   idempotent; a second pass completes identically).
//!
//! The dispatch seams (KP-1…KP-6) live in `hh-env::dispatch` — the same
//! enum is the shared vocabulary; each injection point is the boundary
//! *after* the named durable row.

use std::fmt;

/// `KillPoint` — the closed R-2.2.3⁰ᶜ set (KP-1…KP-21; KP-F1…KP-F5 land
/// with the fleet slices they belong to). KP-14/16…21 are the C1/Stage-4
/// parent/child rows (S4.6's spawn/merge slice).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KillPoint {
    /// After `action.effect.intended`, before `security.permission.decided`.
    Kp1,
    /// After `decided{allow}`, before `action.effect.prepared`.
    Kp2,
    /// After `prepared`, before `committed` (or the `read_only` dispatch).
    Kp3,
    /// After `committed` durable, before the dispatch reached the executor.
    Kp4,
    /// During execution — the executor ran, the runtime died mid-wait.
    Kp5,
    /// After execution, before `action.effect.observed` was durable.
    Kp6,
    /// After `observed` durable, before visible/charged.
    Kp7,
    /// After visible, before the next `control.decision`.
    Kp8,
    /// Mid-batch append — the batch is absent as a unit.
    Kp9,
    /// After `suspended`, before `lease.released` (C1 — Stage 2 slice).
    Kp10,
    /// After `occurred`, before `fired` (C1 — Stage 2 slice).
    Kp11,
    /// During `heal` (C1 — Stage 2 slice).
    Kp12,
    /// During `restore` — recovery is idempotent; a second pass completes.
    Kp13,
    /// A child run while the parent is active (C1 — Stage 4 slice).
    Kp14,
    /// Helper death with the runtime alive (the `executor_error` /
    /// `environment_replaced` unknown-cause path).
    Kp15,
    /// Parent stop with running children — every `on_parent_end = cancel`
    /// child is `cancelled{parent_stop}` within `TimeoutPolicy[subagent].hard_max`
    /// (C1 — Stage 4 slice; AC-F3-06).
    Kp16,
    /// Child killed mid-effect — the child restores under its own lease; the
    /// parent waits ≤ the `subagent` scope deadline; no duplicate effect
    /// (C1 — Stage 4 slice).
    Kp17,
    /// Child exhausts while the parent is also spent — the child stops with
    /// the parent's `Exceeded` (root-first, C-6).
    Kp18,
    /// Parent handle revoked mid-run — the child's next covered proposal is
    /// `NoCoveringGrant` (cascade revocation, C-7).
    Kp19,
    /// Parent and child killed — both restore under their own leases; the
    /// child's terminal reaches the parent's resubscribed `child_terminal`
    /// subscription (C-3/C-4).
    Kp20,
    /// Kill between the `spawns` reservation and `control.subagent.spawned` —
    /// nothing is held (reservations die with the lease); `spawn` re-runs
    /// idempotently (C1 — Stage 4 slice).
    Kp21,
}

impl KillPoint {
    /// The canonical spelling (`kp-1`, …).
    pub fn as_str(self) -> &'static str {
        match self {
            KillPoint::Kp1 => "kp-1",
            KillPoint::Kp2 => "kp-2",
            KillPoint::Kp3 => "kp-3",
            KillPoint::Kp4 => "kp-4",
            KillPoint::Kp5 => "kp-5",
            KillPoint::Kp6 => "kp-6",
            KillPoint::Kp7 => "kp-7",
            KillPoint::Kp8 => "kp-8",
            KillPoint::Kp9 => "kp-9",
            KillPoint::Kp10 => "kp-10",
            KillPoint::Kp11 => "kp-11",
            KillPoint::Kp12 => "kp-12",
            KillPoint::Kp13 => "kp-13",
            KillPoint::Kp14 => "kp-14",
            KillPoint::Kp15 => "kp-15",
            KillPoint::Kp16 => "kp-16",
            KillPoint::Kp17 => "kp-17",
            KillPoint::Kp18 => "kp-18",
            KillPoint::Kp19 => "kp-19",
            KillPoint::Kp20 => "kp-20",
            KillPoint::Kp21 => "kp-21",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<KillPoint> {
        Some(match s {
            "kp-1" | "kp1" => KillPoint::Kp1,
            "kp-2" | "kp2" => KillPoint::Kp2,
            "kp-3" | "kp3" => KillPoint::Kp3,
            "kp-4" | "kp4" => KillPoint::Kp4,
            "kp-5" | "kp5" => KillPoint::Kp5,
            "kp-6" | "kp6" => KillPoint::Kp6,
            "kp-7" | "kp7" => KillPoint::Kp7,
            "kp-8" | "kp8" => KillPoint::Kp8,
            "kp-9" | "kp9" => KillPoint::Kp9,
            "kp-10" | "kp10" => KillPoint::Kp10,
            "kp-11" | "kp11" => KillPoint::Kp11,
            "kp-12" | "kp12" => KillPoint::Kp12,
            "kp-13" | "kp13" => KillPoint::Kp13,
            "kp-14" | "kp14" => KillPoint::Kp14,
            "kp-15" | "kp15" => KillPoint::Kp15,
            "kp-16" | "kp16" => KillPoint::Kp16,
            "kp-17" | "kp17" => KillPoint::Kp17,
            "kp-18" | "kp18" => KillPoint::Kp18,
            "kp-19" | "kp19" => KillPoint::Kp19,
            "kp-20" | "kp20" => KillPoint::Kp20,
            "kp-21" | "kp21" => KillPoint::Kp21,
            _ => return None,
        })
    }

    /// The C0/Stage-3 battery set — KP-1…KP-9, KP-13, KP-15 (KP-10/11/12
    /// are the C1 suspend/wakeup/heal slice landed at Stage 2; KP-14 is the
    /// parent/child slice; KP-16…21/KP-F* are later chain tickets).
    pub const STAGE3_SET: &'static [KillPoint] = &[
        KillPoint::Kp1,
        KillPoint::Kp2,
        KillPoint::Kp3,
        KillPoint::Kp4,
        KillPoint::Kp5,
        KillPoint::Kp6,
        KillPoint::Kp7,
        KillPoint::Kp8,
        KillPoint::Kp9,
        KillPoint::Kp13,
        KillPoint::Kp15,
    ];

    /// The C1/Stage-4 parent/child battery set — KP-14, KP-16…KP-21
    /// (ADR-0132 battery rows in ADR-0208's numbering, CF-459; S4.6).
    pub const STAGE4_SET: &'static [KillPoint] = &[
        KillPoint::Kp14,
        KillPoint::Kp16,
        KillPoint::Kp17,
        KillPoint::Kp18,
        KillPoint::Kp19,
        KillPoint::Kp20,
        KillPoint::Kp21,
    ];
}

impl fmt::Display for KillPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kill_point_spelling_round_trips() {
        for kp in KillPoint::STAGE3_SET {
            assert_eq!(KillPoint::parse(kp.as_str()), Some(*kp));
        }
        assert_eq!(KillPoint::parse("kp-99"), None);
    }
}
