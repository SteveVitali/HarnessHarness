//! Goals, continuation chains and inbox runs (§5a.3 — `R-2.2.3` C1·S4;
//! ADR-0131 §5; S4.13).
//!
//! A **goal** is a durable coordinate (`goal_ref`) its activations aggregate
//! under. An activation is an `agent` run carrying `goal_ref` +
//! `activation_no`; `continue_goal` opens the *next* activation under
//! `continued_from{run_id, at_seq, head_hash}` — a **non-diverging** fork
//! (a second continuation from the same head is `AlreadyContinued`, never a
//! silent sibling). The `lifecycle.run.created` manifest is the record —
//! `carried` rides `manifest.extra` so the carried `{resume_set_heads,
//! budget_id}` is durable with the activation, never a copied transcript.
//!
//! The **inbox run** (`run_kind = inbox`, `goal_ref` required by manifest
//! validation) is where a goal with no open activation receives its
//! occurrences: goal-scoped subscriptions are scheduled on the inbox run
//! (`subscription.owner = goal:<ref>` — sleep/wake per goal, not per
//! activation) and survive `continue_goal` untouched — the next activation
//! reads them from the inbox run, nothing moves.
//!
//! Unattended defaults (§5a.3): inbox runs mint `attendance = unattended`;
//! `schedule`/`external` on an interactive activation refuses without a
//! reachable principal — [`crate::wakeup`] owns that refusal.

use hh_wire::json::Json;

use crate::errors::LedgerError;
use crate::manifest::{LineageLink, RunKind, RunManifest};
use crate::store::{Lease, Store};
use crate::wakeup::WakeupSubscription;

/// `ContinueCarried` — what the next activation inherits (§5a.3
/// `continue_goal(goal_ref, from_run, carried{resume_set_heads, budget_id},
/// manifest_delta)`): the §05c `resume_set` heads and the goal-scoped
/// `BudgetNode` — references, never a copied transcript.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContinueCarried {
    /// The §05c `resume_set` heads carried into the next activation.
    pub resume_set_heads: Vec<String>,
    /// The goal-scoped `BudgetNode` id (ADR-0040 `scope: Goal`); when set it
    /// becomes the activation's `budget` member.
    pub budget_id: Option<String>,
}

impl ContinueCarried {
    /// The canonical `carried` member the new manifest records.
    pub fn to_json(&self) -> Json {
        Json::obj([
            (
                "resume_set_heads",
                Json::Arr(self.resume_set_heads.iter().map(Json::str).collect()),
            ),
            (
                "budget_id",
                match &self.budget_id {
                    Some(b) => Json::str(b.clone()),
                    None => Json::Null,
                },
            ),
        ])
    }
}

/// The outcome of [`Store::continue_goal`] — the new activation's id and
/// writer lease (the durable links are on the `created` manifest).
#[derive(Debug, Clone)]
pub struct Continuation {
    /// The new activation.
    pub run_id: String,
    /// Its writer lease.
    pub lease: Lease,
    /// `activation_no` the continuation carries (`from + 1`).
    pub activation_no: u64,
}

impl Store {
    /// `open_inbox(goal_ref, holder)` — the goal's inbox run
    /// (`run_kind = inbox`; no configuration, environment or metric cells —
    /// ADR-0183 §C). Idempotence is the caller's lookup
    /// ([`Store::inbox_for_goal`]) — two inboxes for one goal are legal but
    /// wasteful; the store does not invent a singleton index.
    pub fn open_inbox(
        &mut self,
        goal_ref: &str,
        holder: &str,
    ) -> Result<(String, Lease), LedgerError> {
        self.tier_c1("open_inbox")?;
        let mut m = RunManifest::minimal(RunKind::Inbox);
        m.goal_ref = Some(goal_ref.to_string());
        // The inbox carries no configuration/environment/metric cells
        // (ADR-0183 §C) — `minimal` pins an agent's configuration, so the
        // non-agent members are cleared before `open_run` validates.
        m.configuration_id = None;
        m.configuration_version_id = None;
        self.open_run(m, holder)
    }

    /// `inbox_for_goal(goal_ref) → run_id` — the goal's inbox run (the first
    /// `run_kind = inbox` run carrying the coordinate; the caller opens one
    /// when absent).
    pub fn inbox_for_goal(&self, goal_ref: &str) -> Result<Option<String>, LedgerError> {
        self.tier_c1("inbox_for_goal")?;
        for run_id in self.runs.keys() {
            let m = self.manifest(run_id)?;
            if m.run_kind == RunKind::Inbox && m.goal_ref.as_deref() == Some(goal_ref) {
                return Ok(Some(run_id.clone()));
            }
        }
        Ok(None)
    }

    /// `continue_goal(goal_ref, from_run, carried, manifest_delta) →
    /// Continuation` — the §5a.3 continuation-chain op.
    ///
    /// - `from_run` must carry `goal_ref`, be finished, and not have an
    ///   explicit terminal outcome (`finished{continue_to: false}` →
    ///   `GoalFinished`).
    /// - The continuation binds `{run_id: from, at_seq: from_head,
    ///   head_hash}` — a *non-diverging* link: a second activation from the
    ///   same head is `AlreadyContinued` (fold across runs — restart-stable).
    /// - `activation_no` is `from.activation_no + 1`; `parent_run_id`/
    ///   `spawn_event` are inherited; `carried` lands on the manifest's
    ///   `extra` (the durable record — refs, never a transcript).
    /// - `manifest_delta` merges over the inherited manifest (caller's
    ///   coordinate deltas — budget, seed, refs); the lineage members are
    ///   then forced (`continued_from`, `activation_no`, `goal_ref` —
    ///   a delta cannot rewrite ancestry).
    /// - `carried.budget_id` replaces `budget` — the goal-scoped
    ///   `BudgetNode` is the activation's budget root (ADR-0040).
    pub fn continue_goal(
        &mut self,
        goal_ref: &str,
        from_run_id: &str,
        carried: &ContinueCarried,
        manifest_delta: Option<&Json>,
        holder: &str,
    ) -> Result<Continuation, LedgerError> {
        self.tier_c1("continue_goal")?;
        let from_manifest = self.manifest(from_run_id)?.clone();
        if from_manifest.goal_ref.as_deref() != Some(goal_ref) {
            return Err(LedgerError::GoalFinished {
                run_id: from_run_id.to_string(),
                detail: format!("manifest.goal_ref does not carry {goal_ref}"),
            });
        }
        if !self.run(from_run_id)?.finished {
            return Err(LedgerError::GoalFinished {
                run_id: from_run_id.to_string(),
                detail: "not_finished".to_string(),
            });
        }
        // An explicit terminal outcome ends the goal (`continue_to: false`
        // — the §5a.3 `activation_finished{continue_to}` member); absent
        // members are pre-S4.13 runs and do not refuse.
        if let Some(finished) = self
            .events(from_run_id)?
            .iter()
            .rev()
            .find(|e| e.class == "lifecycle.run.finished")
        {
            let is_false = |v: Option<&Json>| matches!(v, Some(Json::Bool(false)));
            let terminal = is_false(finished.payload.get("continue_to"))
                || is_false(
                    finished
                        .payload
                        .get("outcome")
                        .and_then(|o| o.get("continue_to")),
                );
            if terminal {
                return Err(LedgerError::GoalFinished {
                    run_id: from_run_id.to_string(),
                    detail: "continue_to: false".to_string(),
                });
            }
        }
        let head_seq = self.events(from_run_id)?.len().saturating_sub(1) as u64;
        // Non-diverging — a `continued_from` naming this head already
        // exists (the fold is over the durable manifests, so a crash and
        // re-open answers the same).
        for run_id in self.runs.keys() {
            if let Some(l) = &self.manifest(run_id)?.continued_from {
                if l.run_id == from_run_id && l.at_seq == head_seq {
                    return Err(LedgerError::AlreadyContinued {
                        run_id: from_run_id.to_string(),
                        at_seq: head_seq,
                        existing: run_id.clone(),
                    });
                }
            }
        }
        // Inherited manifest + caller delta; the lineage members are forced
        // after the merge (a delta names the new run's coordinates, never
        // its ancestry — AuthorityWidening-adjacent honesty).
        let mut base = from_manifest.to_json();
        if let Some(Json::Obj(delta)) = manifest_delta {
            if let Json::Obj(b) = &mut base {
                for (k, v) in delta {
                    b.insert(k.clone(), v.clone());
                }
            }
        }
        let mut manifest = RunManifest::from_json(&base)?;
        manifest.forked_from = None;
        manifest.continued_from = Some(LineageLink {
            run_id: from_run_id.to_string(),
            at_seq: head_seq,
            head_hash: self
                .events(from_run_id)?
                .last()
                .map(|e| e.hash.clone())
                .unwrap_or_default(),
        });
        manifest.activation_no = from_manifest.activation_no + 1;
        manifest.goal_ref = Some(goal_ref.to_string());
        manifest.parent_run_id = from_manifest.parent_run_id.clone();
        manifest.spawn_event = from_manifest.spawn_event.clone();
        // The goal's non-agent cells stay absent — a continuation is an
        // `agent` activation (inbox/fleet/experiment kinds never continue).
        if manifest.run_kind != RunKind::Agent {
            manifest.run_kind = RunKind::Agent;
        }
        if let Some(b) = &carried.budget_id {
            manifest.budget = Some(b.clone());
        }
        manifest
            .extra
            .insert("carried".to_string(), carried.to_json());
        let (run_id, lease) = self.open_run(manifest, holder)?;
        Ok(Continuation {
            run_id,
            lease,
            activation_no: from_manifest.activation_no + 1,
        })
    }

    /// `goal_subscriptions(goal_ref)` — the goal-scoped subscription set:
    /// every `wsub` on the goal's inbox run (the durable home of
    /// `owner = goal:*` subscriptions — they survive `continue_goal`
    /// because they were never on the activation).
    pub fn goal_subscriptions(
        &self,
        goal_ref: &str,
    ) -> Result<Vec<WakeupSubscription>, LedgerError> {
        match self.inbox_for_goal(goal_ref)? {
            Some(inbox) => self.wakeup_subscriptions(&inbox),
            None => Ok(Vec::new()),
        }
    }

    /// `activation_chain(goal_ref) → [run_id]` — the goal's activations in
    /// `activation_no` order (the continuation chain's audit projection).
    pub fn activation_chain(&self, goal_ref: &str) -> Result<Vec<String>, LedgerError> {
        let mut chain: Vec<(u64, String)> = Vec::new();
        for run_id in self.runs.keys() {
            let m = self.manifest(run_id)?;
            if m.run_kind == RunKind::Agent && m.goal_ref.as_deref() == Some(goal_ref) {
                chain.push((m.activation_no, run_id.clone()));
            }
        }
        chain.sort();
        Ok(chain.into_iter().map(|(_, id)| id).collect())
    }

    /// `subscribe` on a goal — the durable `control.wakeup.scheduled` lands
    /// on the goal's *inbox run* with `owner = goal:<ref>` (the
    /// goal-scoped form; §5a.3 "goal-scoped subscriptions (sleep/wake per
    /// goal, not per activation)"). The inbox run must exist —
    /// [`Store::open_inbox`] creates it; the lease is the inbox's own.
    pub fn goal_subscribe(
        &mut self,
        goal_ref: &str,
        inbox_run_id: &str,
        lease: &Lease,
        trigger: crate::wakeup::Trigger,
        policy: crate::wakeup::WakeupPolicy,
        created_by: &crate::manifest::EventRef,
    ) -> Result<String, LedgerError> {
        self.tier_c1("goal_subscribe")?;
        let inbox = self.manifest(inbox_run_id)?;
        if inbox.run_kind != RunKind::Inbox || inbox.goal_ref.as_deref() != Some(goal_ref) {
            return Err(LedgerError::SchemaViolation {
                detail: format!(
                    "goal_subscribe: {inbox_run_id} is not the inbox run for {goal_ref}"
                ),
            });
        }
        self.wakeup_subscribe_owned(
            inbox_run_id,
            lease,
            &format!("goal:{goal_ref}"),
            trigger,
            policy,
            created_by,
        )
    }

    /// `goal_occurred(goal_ref, …)` — record an occurrence against one of
    /// the goal's inbox subscriptions: "a goal with no open activation
    /// receives it in its inbox run" (§5a.3). The lease is the inbox's own
    /// writer (the ingress adapter holds it — the durable `occurred` row is
    /// the only path in).
    pub fn goal_occurred(
        &mut self,
        goal_ref: &str,
        lease: &Lease,
        subscription_id: &str,
        occurrence_key: &str,
        payload_ref: Option<&str>,
        observed_at_ms: u64,
    ) -> Result<crate::wakeup::OccurOutcome, LedgerError> {
        let inbox = self
            .inbox_for_goal(goal_ref)?
            .ok_or_else(|| LedgerError::UnknownRun {
                run_id: format!("inbox for {goal_ref}"),
            })?;
        self.wakeup_occurred(
            &inbox,
            lease,
            subscription_id,
            occurrence_key,
            payload_ref,
            observed_at_ms,
        )
    }
}
