//! `suspend` (§5a.3; ADR-0131 §3 — `R-2.2.3` C1·S2): the declared pause. A run
//! suspends only with **no open committed work** — `S-1: no effect in
//! prepared/deferred/committed`, refused `OpenCommittedEffects` — then appends
//! `lifecycle.run.suspended{reasons[], subscription_ids[], resume_policy}` and
//! (default) releases the writer lease (`lifecycle.lease.released{reason:
//! suspended}`). The suspended run is exempt from liveness-based takeover and
//! stall restart — the `suspended` fold fact is what `acquire_writer`'s
//! probe-shortening reads.
//!
//! KP-10 (crash between `suspended` and `lease.released`): the next `restore`
//! completes the release — see [`crate::recovery`]; a suspended-but-leased run
//! is still taken over only on real expiry (never on probe).

use hh_wire::json::Json;

use crate::effect::EffectPhase;
use crate::errors::LedgerError;
use crate::event::Scope;
use crate::recovery::kernel_ev;
use crate::store::{Lease, Store};

/// `SuspendReason` — the closed sum (§5a.3 suspend row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuspendReason {
    /// Waiting on a permission decision (`awaiting_approval{permission_id}`).
    AwaitingApproval {
        /// The owed permission.
        permission_id: String,
    },
    /// Waiting on a wakeup subscription occurrence.
    AwaitingEvent {
        /// The subscription.
        subscription_id: String,
    },
    /// Waiting on a timer.
    AwaitingTimer {
        /// The wake instant (RFC 3339 ms or wall-ms spelling).
        at: String,
    },
    /// Waiting on a child run's terminal.
    AwaitingChild {
        /// The child.
        child_run_id: String,
    },
    /// Waiting on an effect's terminal.
    AwaitingEffect {
        /// The effect.
        effect_id: String,
    },
    /// Waiting on the environment (`action.environment.ready`).
    AwaitingEnvironment {
        /// The handle.
        env_handle_id: String,
    },
    /// An operator pause.
    OperatorPause,
    /// A hibernation suspend (the §5a.3 hibernated member).
    Hibernated,
}

impl SuspendReason {
    /// The ledger spelling `{type, …}`.
    pub fn to_json(&self) -> Json {
        match self {
            SuspendReason::AwaitingApproval { permission_id } => Json::obj([
                ("type", Json::str("awaiting_approval")),
                ("permission_id", Json::str(permission_id)),
            ]),
            SuspendReason::AwaitingEvent { subscription_id } => Json::obj([
                ("type", Json::str("awaiting_event")),
                ("subscription_id", Json::str(subscription_id)),
            ]),
            SuspendReason::AwaitingTimer { at } => {
                Json::obj([("type", Json::str("awaiting_timer")), ("at", Json::str(at))])
            }
            SuspendReason::AwaitingChild { child_run_id } => Json::obj([
                ("type", Json::str("awaiting_child")),
                ("child_run_id", Json::str(child_run_id)),
            ]),
            SuspendReason::AwaitingEffect { effect_id } => Json::obj([
                ("type", Json::str("awaiting_effect")),
                ("effect_id", Json::str(effect_id)),
            ]),
            SuspendReason::AwaitingEnvironment { env_handle_id } => Json::obj([
                ("type", Json::str("awaiting_environment")),
                ("env_handle_id", Json::str(env_handle_id)),
            ]),
            SuspendReason::OperatorPause => Json::obj([("type", Json::str("operator_pause"))]),
            SuspendReason::Hibernated => Json::obj([("type", Json::str("hibernated"))]),
        }
    }
}

/// What `suspend` committed (the report; the rows are the record).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuspendOutcome {
    /// The `lifecycle.run.suspended` seq range.
    pub suspended: crate::event::SeqRange,
    /// Whether the writer lease was released (`release_lease`).
    pub released_lease: bool,
}

impl Store {
    /// `suspend(run, lease, reasons, subscription_ids, release_lease = true)` —
    /// S-1 enforced: any effect in `prepared`/`deferred`/`committed` refuses
    /// `OpenCommittedEffects`. `lifecycle.run.suspended` lands first, then the
    /// lease release — a crash between the two is KP-10, completed by the next
    /// `restore`.
    pub fn suspend(
        &mut self,
        run_id: &str,
        lease: &Lease,
        reasons: &[SuspendReason],
        subscription_ids: &[String],
        resume_policy: Json,
        release_lease: bool,
    ) -> Result<SuspendOutcome, LedgerError> {
        self.tier_c1("suspend")?;
        // S-1 — no open committed work.
        let open: Vec<String> = self
            .run(run_id)?
            .effects
            .values()
            .filter(|f| {
                matches!(
                    f.phase,
                    EffectPhase::Prepared | EffectPhase::Deferred | EffectPhase::Committed
                )
            })
            .map(|f| f.effect_id.clone())
            .collect();
        if !open.is_empty() {
            return Err(LedgerError::OpenCommittedEffects { effect_ids: open });
        }
        let ev = kernel_ev(
            self,
            run_id,
            "lifecycle.run.suspended",
            Scope::default(),
            Json::obj([
                (
                    "reasons",
                    Json::Arr(reasons.iter().map(SuspendReason::to_json).collect()),
                ),
                (
                    "subscription_ids",
                    Json::Arr(
                        subscription_ids
                            .iter()
                            .map(|s| Json::str(s.clone()))
                            .collect(),
                    ),
                ),
                ("resume_policy", resume_policy),
            ]),
        )?;
        let suspended = self.append(run_id, lease, vec![ev])?;
        if release_lease {
            self.release(lease, "suspended")?;
        }
        Ok(SuspendOutcome {
            suspended,
            released_lease: release_lease,
        })
    }

    /// `is_suspended(run)` — the fold fact (`suspended` … `resumed`/`finished`).
    pub fn is_suspended(&self, run_id: &str) -> Result<bool, LedgerError> {
        Ok(self.run(run_id)?.suspended)
    }
}
