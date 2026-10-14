//! The S4.13 durability ops (§5a.3/§5a.4 surfaces; ADR-0131…0134) —
//! intra-run branches (`branch.open` + the `promote`/`discard` verbs the
//! op registry staged at Stage 1), `continue_goal`/`open_inbox` (the goal
//! continuation chain + inbox runs), and the wakeup `subscribe`/
//! `record_occurrence` pair (`schedule`/`external`/`peer_message`
//! triggers — durable before any run acts).
//!
//! Boundary rules these honor:
//! - **Writer sessions only** — every op appends under the session's
//!   fenced lease (or the service holder for `open_inbox`); an attach
//!   session is `session_is_read_only`.
//! - **R-NOSIDE** — branch ids and subscription ids are kernel
//!   coordinates returned to the host (they are the op's *result*, like
//!   `fork`'s `branch_id`); env handle ids never leave.
//! - **Typed refusals, never coerced** — `AuthorityWidening` (a branch
//!   claiming a permission the parent lacks, or a `SpeculationPolicy`
//!   widening the `defer_irreversible` floor), `AlreadyContinued`,
//!   `TriggerUnsupported`, `SpeculationViolation` all surface verbatim
//!   through `ledger_err`.

use hh_embed_schema::errors::EmbedError;
use hh_ledger::branch_ops::{OpenBranchSpec, ReleaseMap};
use hh_ledger::effect::EffectPhase;
use hh_ledger::goal::ContinueCarried;
use hh_ledger::manifest::EventRef;
use hh_ledger::store::Lease;
use hh_ledger::suspend::SuspendReason;
use hh_ledger::wakeup::{
    SubscriptionState, Trigger, WakeupPolicy, WakeupSubscription, WokenDelivery,
};
use hh_wire::json::Json;

use crate::service::{ledger_err, EmbedService};

impl EmbedService {
    /// The shared write-op subject: a live *writer* session's
    /// `(run_id, lease)` — no env requirement (branches and wakeups are
    /// run-scoped).
    fn writer_subject(&mut self, params: &Json, op: &str) -> Result<(String, Lease), EmbedError> {
        let session_id = params
            .get("session_id")
            .and_then(Json::as_str)
            .ok_or_else(|| EmbedError::SchemaViolation {
                path: format!("{op}/session_id"),
                code: "missing".to_string(),
            })?
            .to_string();
        let s = self.writer_session(&session_id)?;
        Ok((
            s.run_id.clone(),
            s.lease.clone().ok_or(EmbedError::Refused {
                reason: "session_is_read_only".to_string(),
            })?,
        ))
    }

    /// `branch.open{session_id, kind?, read_only?, policy?,
    /// budget_slice_id?, fork_seq?, permissions?}` — the C2 intra-run
    /// branch (§5a.4 `fork` intra arm; ADR-0133/0134): a coherent fork
    /// point, the `SpeculationPolicy` record verbatim, and containment at
    /// `fork` — a permission claim the parent does not hold, or a policy
    /// widening `defer_irreversible`, is `AuthorityWidening`
    /// (AC-R-2.2.4-10), never coerced.
    pub(crate) fn branch_open(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, lease) = self.writer_subject(params, "branch.open")?;
        let spec = OpenBranchSpec {
            kind: params
                .get("kind")
                .and_then(Json::as_str)
                .unwrap_or("speculative")
                .to_string(),
            fork_seq: params
                .get("fork_seq")
                .and_then(Json::as_int)
                .map(|s| s.max(0) as u64),
            read_only: params.get("read_only") == Some(&Json::Bool(true)),
            policy: params.get("policy").cloned(),
            budget_slice_id: params
                .get("budget_slice_id")
                .and_then(Json::as_str)
                .map(str::to_string),
            env_binding: params
                .get("env_binding")
                .and_then(Json::as_str)
                .map(str::to_string),
            evidence_path: params
                .get("evidence_path")
                .and_then(Json::as_str)
                .map(str::to_string),
            permissions: params
                .get("permissions")
                .and_then(|p| match p {
                    Json::Arr(v) => Some(
                        v.iter()
                            .filter_map(Json::as_str)
                            .map(str::to_string)
                            .collect(),
                    ),
                    _ => None,
                })
                .unwrap_or_default(),
            extra: vec![],
        };
        let branch_id = self
            .store
            .open_branch(&run_id, &lease, &spec)
            .map_err(ledger_err)?;
        Ok(Json::obj([
            ("branch_id", Json::str(branch_id)),
            ("kind", Json::str(spec.kind)),
            ("read_only", Json::Bool(spec.read_only)),
        ]))
    }

    /// `promote{session_id, branch_id, releases?}` — merge the branch:
    /// every deferred effect is released `allow`/`deny` per `releases`
    /// (absent entries default `deny` — §5a.4's explicit-release rule;
    /// a refused release is `refused`, never fatal to the promote).
    pub(crate) fn branch_promote(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, lease) = self.writer_subject(params, "promote")?;
        let branch_id = params
            .get("branch_id")
            .and_then(Json::as_str)
            .ok_or_else(|| EmbedError::SchemaViolation {
                path: "promote/branch_id".into(),
                code: "missing".into(),
            })?
            .to_string();
        let releases: ReleaseMap = params
            .get("releases")
            .and_then(|r| match r {
                Json::Obj(m) => Some(m),
                _ => None,
            })
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| match v {
                        Json::Str(s) => Some((k.clone(), s == "allow")),
                        Json::Bool(b) => Some((k.clone(), *b)),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.store
            .promote_branch(&run_id, &lease, &branch_id, &releases)
            .map_err(ledger_err)?;
        Ok(Json::obj([
            ("branch_id", Json::str(branch_id)),
            ("promoted", Json::Bool(true)),
        ]))
    }

    /// `discard{session_id, branch_id}` — drop the branch: HEAD rewinds
    /// to the fork point, branch effects terminate per the §5a.4 table.
    /// The embed boundary runs **no compensator dispatch** — a
    /// `compensable` intent lands `compensation_failed{boundary:
    /// no_dispatch}` on the record (the honest `n/a` at this seam — the
    /// saga path stays with the orchestrator).
    pub(crate) fn branch_discard(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, lease) = self.writer_subject(params, "discard")?;
        let branch_id = params
            .get("branch_id")
            .and_then(Json::as_str)
            .ok_or_else(|| EmbedError::SchemaViolation {
                path: "discard/branch_id".into(),
                code: "missing".into(),
            })?
            .to_string();
        let outcome = self
            .store
            .discard_branch(&run_id, &lease, &branch_id, &[], None, &mut |_intent| {
                Err("no_compensator_at_boundary".to_string())
            })
            .map_err(ledger_err)?;
        Ok(Json::obj([
            ("branch_id", Json::str(branch_id)),
            ("disposition", Json::str("discarded")),
            (
                "refused",
                Json::Arr(outcome.refused.iter().map(Json::str).collect()),
            ),
            (
                "compensated",
                Json::Arr(outcome.compensated.iter().map(Json::str).collect()),
            ),
            (
                "uncompensable",
                Json::Arr(outcome.uncompensable.iter().map(Json::str).collect()),
            ),
        ]))
    }

    /// `continue_goal{session_id, goal_ref, carried?, manifest_delta?}` —
    /// the goal continuation chain (§5a.3; ADR-0131 §5): opens the next
    /// activation with `continued_from{run_id, at_seq, head_hash}` —
    /// `AlreadyContinued` guards the head; the session's run must carry
    /// `goal_ref` and be finished (the ledger checks; CC3).
    pub(crate) fn continue_goal(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, _lease) = self.writer_subject(params, "continue_goal")?;
        let goal_ref = params
            .get("goal_ref")
            .and_then(Json::as_str)
            .ok_or_else(|| EmbedError::SchemaViolation {
                path: "continue_goal/goal_ref".into(),
                code: "missing".into(),
            })?
            .to_string();
        let carried = ContinueCarried {
            resume_set_heads: params
                .get("carried")
                .and_then(|c| c.get("resume_set_heads"))
                .and_then(|v| match v {
                    Json::Arr(a) => Some(
                        a.iter()
                            .filter_map(Json::as_str)
                            .map(str::to_string)
                            .collect(),
                    ),
                    _ => None,
                })
                .unwrap_or_default(),
            budget_id: params
                .get("carried")
                .and_then(|c| c.get("budget_id"))
                .and_then(Json::as_str)
                .map(str::to_string),
        };
        let holder = self.holder.clone();
        let cont = self
            .store
            .continue_goal(
                &goal_ref,
                &run_id,
                &carried,
                params.get("manifest_delta"),
                &holder,
            )
            .map_err(ledger_err)?;
        Ok(Json::obj([
            ("run_id", Json::str(cont.run_id)),
            ("activation_no", Json::Int(cont.activation_no as i64)),
            ("goal_ref", Json::str(goal_ref)),
        ]))
    }

    /// `open_inbox{goal_ref}` — the goal's inbox run (`run_kind = inbox`;
    /// ADR-0183 §C): goal-scoped subscriptions outlive individual
    /// activations. The op is session-free — the holder is the service's.
    pub(crate) fn open_inbox(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let goal_ref = params
            .get("goal_ref")
            .and_then(Json::as_str)
            .ok_or_else(|| EmbedError::SchemaViolation {
                path: "open_inbox/goal_ref".into(),
                code: "missing".into(),
            })?
            .to_string();
        let holder = self.holder.clone();
        let (run_id, _lease) = self
            .store
            .open_inbox(&goal_ref, &holder)
            .map_err(ledger_err)?;
        Ok(Json::obj([
            ("run_id", Json::str(run_id)),
            ("goal_ref", Json::str(goal_ref)),
            ("run_kind", Json::str("inbox")),
        ]))
    }

    /// `subscribe{session_id, trigger, policy?}` — the §5a.4 wakeup
    /// subscription (`schedule`/`external`/`peer_message` land at S4.13 —
    /// `control.wakeup.scheduled` under the session lease).
    pub(crate) fn wakeup_subscribe(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, lease) = self.writer_subject(params, "subscribe")?;
        let trigger =
            Trigger::from_json(params.get("trigger").unwrap_or(&Json::Null)).ok_or_else(|| {
                EmbedError::SchemaViolation {
                    path: "subscribe/trigger".into(),
                    code: "unknown_trigger".into(),
                }
            })?;
        let policy = WakeupPolicy::from_json(params.get("policy").unwrap_or(&Json::obj([])))
            .ok_or_else(|| EmbedError::SchemaViolation {
                path: "subscribe/policy".into(),
                code: "invalid_policy".into(),
            })?;
        let created_by = EventRef {
            run_id: run_id.clone(),
            event_id: self.store.head(&run_id).map_err(ledger_err)?.event_id,
        };
        let subscription_id = self
            .store
            .wakeup_subscribe(&run_id, &lease, trigger, policy, &created_by)
            .map_err(ledger_err)?;
        Ok(Json::obj([("subscription_id", Json::str(subscription_id))]))
    }

    /// `record_occurrence{session_id, subscription_id, occurrence_key,
    /// payload_ref?, observed_at_ms?}` — the §5a.4 occurrence record:
    /// `control.wakeup.occurred` is durable before any run acts; a
    /// duplicate key is `skipped{duplicate_occurrence}`, audited, never a
    /// second fire.
    pub(crate) fn record_occurrence(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, lease) = self.writer_subject(params, "record_occurrence")?;
        let subscription_id = params
            .get("subscription_id")
            .and_then(Json::as_str)
            .ok_or_else(|| EmbedError::SchemaViolation {
                path: "record_occurrence/subscription_id".into(),
                code: "missing".into(),
            })?
            .to_string();
        let occurrence_key = params
            .get("occurrence_key")
            .and_then(Json::as_str)
            .ok_or_else(|| EmbedError::SchemaViolation {
                path: "record_occurrence/occurrence_key".into(),
                code: "missing".into(),
            })?
            .to_string();
        let observed_at_ms = params
            .get("observed_at_ms")
            .and_then(Json::as_int)
            .map(|v| v.max(0) as u64)
            .unwrap_or_else(|| self.store.now_ms());
        let outcome = self
            .store
            .wakeup_occurred(
                &run_id,
                &lease,
                &subscription_id,
                &occurrence_key,
                params.get("payload_ref").and_then(Json::as_str),
                observed_at_ms,
            )
            .map_err(ledger_err)?;
        Ok(Json::obj([
            ("subscription_id", Json::str(subscription_id)),
            ("occurrence_key", Json::str(occurrence_key)),
            (
                "outcome",
                Json::str(match outcome {
                    hh_ledger::wakeup::OccurOutcome::Occurred(_) => "occurred",
                    hh_ledger::wakeup::OccurOutcome::Skipped(_) => "skipped",
                }),
            ),
        ]))
    }

    /// `deliver_due_wakeups(session_id) → Vec<WokenDelivery>` — the S5.8
    /// surface drain seam (R-2.2.3²; §5a.3 wakeup table): runs the
    /// kernel-internal trigger pass (`deliver_wakeup` — due occurrences
    /// land `control.wakeup.occurred` durable-first, then `fired` under
    /// the W-1 claim) under the session's writer lease, then returns the
    /// deliveries this session has not yet consumed. `delivered_wokens`
    /// is the same dedup key the run-loop drain uses — a surface poll
    /// never re-delivers what a decision point already cued, and a
    /// `fired` row a dead surface never read re-surfaces on the next
    /// call (at-least-once; the durable `fired` row is the record, the
    /// set is caller-side bookkeeping).
    ///
    /// Not a registered op — the seam is for surfaces holding the run's
    /// writer session *in process* (binding (a): the MCP server's
    /// `notifications/tasks`/`resources/updated` push pass); a remote
    /// caller's view of the same fact is `read_ledger` over
    /// `control.wakeup.fired` rows.
    pub fn deliver_due_wakeups(
        &mut self,
        session_id: &str,
    ) -> Result<Vec<WokenDelivery>, EmbedError> {
        let (run_id, lease) = {
            let s = self.writer_session(session_id)?;
            (
                s.run_id.clone(),
                s.lease.clone().ok_or_else(|| EmbedError::Refused {
                    reason: "session_is_read_only".to_string(),
                })?,
            )
        };
        let now = self.store.now_ms();
        self.store
            .deliver_wakeup(&run_id, &lease, now)
            .map_err(ledger_err)?;
        let drained = self.store.wakeup_drain(&run_id).map_err(ledger_err)?;
        let mut fresh = Vec::new();
        for w in drained {
            // `steer` deliveries are the `steer{mode: next_turn}` op's
            // durable queue — they enter `decide` through `drive`, never a
            // surface poll (the caller-side `delivered_wokens` set is
            // shared, so draining one here would swallow the cue).
            if w.delivery_mode == hh_ledger::wakeup::DeliveryMode::Steer {
                continue;
            }
            let key = format!("{}\u{0}{}", w.subscription_id, w.occurrence_key);
            if self.session_mut(session_id)?.delivered_wokens.insert(key) {
                fresh.push(w);
            }
        }
        Ok(fresh)
    }

    // ── env suspend/resume (the session's handle; env_subject carries
    // the env_handle_id) ─────────────────────────────────────────────

    /// `env.suspend{session_id, on_idle?}` — `ready → suspended`
    /// (§5a.5; the class's `suspend` capability gates —
    /// `SuspendKind::Unknown` is the honest `Unsupported`).
    pub(crate) fn env_suspend(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, lease, env_handle_id) = self.env_subject(params, "env.suspend")?;
        let on_idle = params.get("on_idle").and_then(Json::as_str);
        let driver = self.env_drivers.get_mut(&run_id).ok_or_else(|| {
            EmbedError::EnvironmentUnavailable {
                reason: "env_driver_absent".to_string(),
            }
        })?;
        // S5.8 (R-2.2.3²) — `on_idle:"hibernate"` is the hibernation-aware
        // suspend: the provider's memory checkpoint is taken inside the
        // same durable batch (`hibernation_snapshot` on the `suspended`
        // row); a class without `snapshot{memory}=supported` keeps the
        // tri-state refusal verbatim.
        if on_idle == Some("hibernate") {
            let rec = driver
                .hibernate(&mut self.store, &lease, &env_handle_id)
                .map_err(crate::open::env_err)?;
            return Ok(Json::obj([
                ("state", Json::str("suspended")),
                ("suspend_reason", Json::str("hibernated")),
                ("hibernation_snapshot", Json::str(rec.snapshot_ref)),
                ("suspended_ms_accruing", Json::Bool(true)),
            ]));
        }
        driver
            .suspend(&mut self.store, &lease, &env_handle_id, on_idle)
            .map_err(crate::open::env_err)?;
        Ok(Json::obj([
            ("state", Json::str("suspended")),
            ("suspended_ms_accruing", Json::Bool(true)),
        ]))
    }

    /// `env.resume{session_id, cause?}` — `suspended → ready`
    /// (`cause ∈ {wakeup, operator}`; the suspended accrual lands on the
    /// `resumed` row).
    pub(crate) fn env_resume(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, lease, env_handle_id) = self.env_subject(params, "env.resume")?;
        let cause = params
            .get("cause")
            .and_then(Json::as_str)
            .unwrap_or("operator");
        if cause != "wakeup" && cause != "operator" {
            return Err(EmbedError::SchemaViolation {
                path: "env.resume/cause".into(),
                code: format!("unknown_cause:{cause}"),
            });
        }
        let driver = self.env_drivers.get_mut(&run_id).ok_or_else(|| {
            EmbedError::EnvironmentUnavailable {
                reason: "env_driver_absent".to_string(),
            }
        })?;
        driver
            .resume(&mut self.store, &lease, &env_handle_id, cause)
            .map_err(crate::open::env_err)?;
        Ok(Json::obj([("state", Json::str("ready"))]))
    }

    // ── R2.3: `suspend`/`compensate`/`heal` — the §5a.3 durable-execution
    // protocol entry points (ADR-0131 §3, ADR-0132 §2; DF-S2.3-1). Every
    // op mints under the session's writer lease; durable rows land before
    // any outcome the caller can observe.
    // ─────────────────────────────────────────────────────────────────

    /// The live-cover test the producing legs share: a subscription
    /// still able to fire for `trigger` (not cancelled, policy expiry
    /// not passed) is the durable promise a new mint would duplicate.
    fn live_sub_id<'a>(
        subs: &'a [WakeupSubscription],
        trigger: &Trigger,
        now_ms: u64,
    ) -> Option<&'a str> {
        subs.iter()
            .find(|s| {
                s.state != SubscriptionState::Cancelled
                    && s.policy.expires_at_ms.is_none_or(|e| now_ms < e)
                    && s.trigger == *trigger
            })
            .map(|s| s.subscription_id.as_str())
    }

    /// `suspend{session_id, reasons[], subscription_ids?, resume_policy?,
    /// release_lease?}` — the run-level `suspend` (§5a.3
    /// `suspend(run, lease, reasons, subscriptions, release_lease =
    /// true)`; ADR-0131 §3). **S-1** refuses typed
    /// `OpenCommittedEffects` while a `prepared`/`deferred`/`committed`
    /// effect is open — checked before any subscription row lands so a
    /// refused suspend leaves no producer residue. Each awaiting-*
    /// reason is a producing leg: `awaiting_timer` mints a `timer`
    /// subscription, `awaiting_approval` a `permission_decided`,
    /// `awaiting_child` a `child_terminal`, `awaiting_effect` an
    /// `effect_terminal` — the `control.wakeup.scheduled` rows are
    /// durable before the `suspended` row itself. `awaiting_event`
    /// names an existing subscription (validated, never minted);
    /// `operator_pause`/`hibernated`/`awaiting_environment` record the
    /// reason only (the `environment_ready` trigger stays the typed
    /// Stage-4 refusal — the wake is the resuming writer's). A released
    /// lease detaches this session — the resume is a new
    /// `open_session`, never a reused stale lease.
    pub(crate) fn run_suspend(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let session_id = params
            .get("session_id")
            .and_then(Json::as_str)
            .ok_or_else(|| EmbedError::SchemaViolation {
                path: "suspend/session_id".into(),
                code: "missing".into(),
            })?
            .to_string();
        let (run_id, lease) = {
            let s = self.writer_session(&session_id)?;
            (
                s.run_id.clone(),
                s.lease.clone().ok_or(EmbedError::Refused {
                    reason: "session_is_read_only".to_string(),
                })?,
            )
        };
        let reasons_json = match params.get("reasons") {
            Some(Json::Arr(a)) => a.clone(),
            _ => {
                return Err(EmbedError::SchemaViolation {
                    path: "suspend/reasons".into(),
                    code: "missing".into(),
                })
            }
        };
        // S-1 pre-check (the store re-checks; the pre-check keeps a
        // refused suspend from minting producer rows it cannot use).
        let folds = self.store.effect_folds(&run_id).map_err(ledger_err)?;
        let open: Vec<String> = folds
            .iter()
            .filter(|(_, f)| {
                matches!(
                    f.phase,
                    EffectPhase::Prepared | EffectPhase::Deferred | EffectPhase::Committed
                )
            })
            .map(|(eid, _)| eid.clone())
            .collect();
        if !open.is_empty() {
            return Err(ledger_err(
                hh_ledger::errors::LedgerError::OpenCommittedEffects { effect_ids: open },
            ));
        }
        let fold_ids: std::collections::BTreeSet<String> =
            folds.iter().map(|(eid, _)| eid.clone()).collect();
        let now = self.store.now_ms();
        let mut reasons: Vec<SuspendReason> = Vec::new();
        let mut triggers: Vec<Trigger> = Vec::new();
        let mut subscription_ids: Vec<String> = Vec::new();
        for (i, r) in reasons_json.iter().enumerate() {
            let path = format!("suspend/reasons[{i}]");
            let bad = |code: &str| EmbedError::SchemaViolation {
                path: path.clone(),
                code: code.to_string(),
            };
            let ty = r
                .get("type")
                .and_then(Json::as_str)
                .ok_or_else(|| bad("missing_type"))?;
            let member = |k: &str| -> Result<String, EmbedError> {
                r.get(k)
                    .and_then(Json::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| bad(&format!("missing_{k}")))
            };
            match ty {
                "awaiting_approval" => {
                    let permission_id = member("permission_id")?;
                    reasons.push(SuspendReason::AwaitingApproval {
                        permission_id: permission_id.clone(),
                    });
                    triggers.push(Trigger::PermissionDecided { permission_id });
                }
                "awaiting_event" => {
                    let subscription_id = member("subscription_id")?;
                    let exists = self
                        .store
                        .wakeup_subscriptions(&run_id)
                        .map_err(ledger_err)?
                        .iter()
                        .any(|s| s.subscription_id == subscription_id);
                    if !exists {
                        return Err(EmbedError::Refused {
                            reason: format!("unknown_subscription:{subscription_id}"),
                        });
                    }
                    reasons.push(SuspendReason::AwaitingEvent { subscription_id });
                }
                "awaiting_timer" => {
                    let (at_ms, spelling) = match r.get("at") {
                        Some(Json::Int(n)) if *n >= 0 => Some((*n as u64, n.to_string())),
                        Some(Json::Str(s)) => s.parse::<u64>().ok().map(|n| (n, s.clone())),
                        _ => None,
                    }
                    .ok_or_else(|| bad("bad_at"))?;
                    reasons.push(SuspendReason::AwaitingTimer { at: spelling });
                    triggers.push(Trigger::Timer { at_ms });
                }
                "awaiting_child" => {
                    let child_run_id = member("child_run_id")?;
                    reasons.push(SuspendReason::AwaitingChild {
                        child_run_id: child_run_id.clone(),
                    });
                    triggers.push(Trigger::ChildTerminal { child_run_id });
                }
                "awaiting_effect" => {
                    let effect_id = member("effect_id")?;
                    if !fold_ids.contains(&effect_id) {
                        return Err(EmbedError::Refused {
                            reason: format!("unknown_effect:{effect_id}"),
                        });
                    }
                    reasons.push(SuspendReason::AwaitingEffect {
                        effect_id: effect_id.clone(),
                    });
                    triggers.push(Trigger::EffectTerminal { effect_id });
                }
                "awaiting_environment" => {
                    // The reason is the record; the `environment_ready`
                    // trigger is the typed Stage-4 refusal at subscribe —
                    // the wake is the resuming writer's `heal`/verify.
                    reasons.push(SuspendReason::AwaitingEnvironment {
                        env_handle_id: member("env_handle_id").unwrap_or_default(),
                    });
                }
                "operator_pause" => reasons.push(SuspendReason::OperatorPause),
                "hibernated" => reasons.push(SuspendReason::Hibernated),
                _ => return Err(bad(&format!("unknown_reason:{ty}"))),
            }
        }
        // The caller-named subscriptions — each must exist (a misspelled
        // id on a suspended run is a promise no producer can keep).
        if let Some(Json::Arr(ids)) = params.get("subscription_ids") {
            let subs = self
                .store
                .wakeup_subscriptions(&run_id)
                .map_err(ledger_err)?;
            for v in ids {
                let id = v.as_str().ok_or_else(|| EmbedError::SchemaViolation {
                    path: "suspend/subscription_ids".into(),
                    code: "type_mismatch".into(),
                })?;
                if !subs.iter().any(|s| s.subscription_id == id) {
                    return Err(EmbedError::Refused {
                        reason: format!("unknown_subscription:{id}"),
                    });
                }
                subscription_ids.push(id.to_string());
            }
        }
        // Mint (or reuse) the producing subscriptions — durable before
        // the `suspended` row.
        for t in triggers {
            let subs = self
                .store
                .wakeup_subscriptions(&run_id)
                .map_err(ledger_err)?;
            if let Some(id) = Self::live_sub_id(&subs, &t, now) {
                subscription_ids.push(id.to_string());
                continue;
            }
            let created_by = EventRef {
                run_id: run_id.clone(),
                event_id: self.store.head(&run_id).map_err(ledger_err)?.event_id,
            };
            subscription_ids.push(
                self.store
                    .wakeup_subscribe(
                        &run_id,
                        &lease,
                        t,
                        WakeupPolicy::default_policy(),
                        &created_by,
                    )
                    .map_err(ledger_err)?,
            );
        }
        let resume_policy = params.get("resume_policy").cloned().unwrap_or(Json::Null);
        let release_lease = params.get("release_lease") != Some(&Json::Bool(false));
        let outcome = self
            .store
            .suspend(
                &run_id,
                &lease,
                &reasons,
                &subscription_ids,
                resume_policy,
                release_lease,
            )
            .map_err(ledger_err)?;
        if outcome.released_lease {
            let s = self.session_mut(&session_id)?;
            s.lease = None;
            s.detached = Some("suspended".to_string());
        }
        Ok(Json::obj([
            (
                "suspended",
                Json::obj([
                    ("first", Json::Int(outcome.suspended.first as i64)),
                    ("last", Json::Int(outcome.suspended.last as i64)),
                ]),
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
            ("released_lease", Json::Bool(outcome.released_lease)),
        ]))
    }

    /// `compensate{session_id, after_seq?, outcomes?}` — the compensation
    /// saga's boundary entry point (§5a.2; ADR-0032): the applied +
    /// `compensable` effects walk in reverse commit order under the
    /// writer lease. `outcomes{<original_effect_id>: <payload>}` is the
    /// host's declared compensator result (`{"error": reason}` names a
    /// failed compensator); a missing entry is the honest
    /// `CompensatorMissing` leg — `abandoned` +
    /// `lifecycle.escalation.raised` land durable, never a fabricated
    /// `observed`. `after_seq` is the rollback-scoped walk verbatim.
    pub(crate) fn run_compensate(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, lease) = self.writer_subject(params, "compensate")?;
        let after_seq = params
            .get("after_seq")
            .and_then(Json::as_int)
            .map(|n| n.max(0) as u64);
        let mut outcomes: std::collections::BTreeMap<String, Result<Json, String>> =
            std::collections::BTreeMap::new();
        if let Some(Json::Obj(m)) = params.get("outcomes") {
            for (eid, v) in m {
                let entry = match v {
                    Json::Obj(o)
                        if o.len() == 1 && o.get("error").is_some_and(|e| e.as_str().is_some()) =>
                    {
                        Err(o
                            .get("error")
                            .and_then(Json::as_str)
                            .unwrap_or_default()
                            .to_string())
                    }
                    other => Ok(other.clone()),
                };
                outcomes.insert(eid.clone(), entry);
            }
        } else if params.get("outcomes").is_some() {
            return Err(EmbedError::SchemaViolation {
                path: "compensate/outcomes".into(),
                code: "type_mismatch".into(),
            });
        }
        let mut dispatch = |intent: &hh_ledger::saga::CompensationIntent| -> Result<Json, String> {
            match outcomes.get(&intent.original_effect_id) {
                Some(Ok(payload)) => Ok(payload.clone()),
                Some(Err(reason)) => Err(reason.clone()),
                None => Err("compensator_missing".to_string()),
            }
        };
        let report = match after_seq {
            Some(at) => self
                .store
                .compensate_run_after(&run_id, &lease, at, &mut dispatch),
            None => self.store.compensate_run(&run_id, &lease, &mut dispatch),
        }
        .map_err(ledger_err)?;
        let arr = |v: &[String]| Json::Arr(v.iter().map(|s| Json::str(s.clone())).collect());
        Ok(Json::obj([
            ("compensated", arr(&report.compensated)),
            ("abandoned", arr(&report.abandoned)),
            ("in_flight", arr(&report.in_flight)),
            ("escalations", arr(&report.escalations)),
        ]))
    }

    /// `heal{session_id, policy_ref?, policy?}` — the policy-bound
    /// environment heal (§5a.3 `heal(run, lease, handle, policy)`;
    /// ADR-0132 §2): the live verdict is *measured*
    /// (`verify_environment_verdict` — never the stale handle), a
    /// non-attached `ready` handle is marked `unreachable` (the durable
    /// `action.environment.unreachable` row), then `heal_with_policy`
    /// runs the ladder — `healed{restored, heal_no}` |
    /// `ask{permission_id}` | `refused{reason}` verbatim. Policy
    /// resolution: `params.policy` inline > `params.policy_ref` > the
    /// run's declared `manifest.healing_policy_ref` > the attendance
    /// default (ADR-0132 §2). A declared ref that does not resolve is a
    /// typed `Refused` — a default is never fabricated over a bad ref.
    /// R-NOSIDE: the env handle never leaves the kernel — the result
    /// reports the outcome members, not the handle id.
    pub(crate) fn run_heal(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, lease, env_handle_id) = self.env_subject(params, "heal")?;
        let policy = self.healing_policy_for(&run_id, params)?;
        let driver = self.env_drivers.get_mut(&run_id).ok_or_else(|| {
            EmbedError::EnvironmentUnavailable {
                reason: "env_driver_absent".to_string(),
            }
        })?;
        let verdict = driver
            .verify_environment_verdict(&mut self.store, &lease, &env_handle_id)
            .map_err(crate::open::env_err)?;
        let cause = match &verdict {
            hh_env::recovery::EnvironmentVerdict::Attached { .. } => {
                return Err(EmbedError::Refused {
                    reason: "environment_attached".to_string(),
                })
            }
            hh_env::recovery::EnvironmentVerdict::Reattachable { .. } => "unreachable".to_string(),
            hh_env::recovery::EnvironmentVerdict::Lost { cause } => cause.as_str().to_string(),
        };
        // `mark_unreachable` rides a `ready` handle only — an already
        // `unreachable` handle goes straight to the ladder; `detached`
        // carries its own reconciliation path (R-2.2.3⁰ᵇ) and a heal is
        // the typed `InvalidState` refusal.
        if driver.handle_state(&env_handle_id) == Some(hh_env::handle::HandleState::Ready) {
            driver
                .mark_unreachable(&mut self.store, &lease, &env_handle_id)
                .map_err(crate::open::env_err)?;
        }
        let outcome = driver
            .heal_with_policy(&mut self.store, &lease, &env_handle_id, &policy, &cause)
            .map_err(crate::open::env_err)?;
        Ok(match outcome {
            hh_env::recovery::HealOutcome::Healed {
                new_handle,
                restored,
                lost_items_ref,
                heal_no,
            } => {
                // The session rebinds to the healed handle — the replaced
                // handle is retired (`action.environment.healed` records
                // the from→to pair); a later `heal`/`env.*` walks the new
                // handle, never the corpse.
                if new_handle != env_handle_id {
                    if let Some(sid) = params.get("session_id").and_then(Json::as_str) {
                        self.session_mut(sid)?.env_handle_id = Some(new_handle);
                    }
                }
                Json::obj([
                    ("outcome", Json::str("healed")),
                    ("restored", Json::str(restored)),
                    ("lost_items_ref", Json::str(lost_items_ref)),
                    ("heal_no", Json::Int(heal_no as i64)),
                ])
            }
            hh_env::recovery::HealOutcome::Ask { permission_id } => Json::obj([
                ("outcome", Json::str("ask")),
                ("permission_id", Json::str(permission_id)),
            ]),
            hh_env::recovery::HealOutcome::Refused { reason } => Json::obj([
                ("outcome", Json::str("refused")),
                ("reason", Json::str(reason)),
            ]),
        })
    }

    /// `healing_policy_ref` resolution — `params.policy` inline >
    /// `params.policy_ref` > `manifest.healing_policy_ref` > the
    /// attendance default (`interactive → ask`, else `reprovision`;
    /// ADR-0132 §2). A declared ref resolves through the one
    /// `RegistryStore`: the record's canonical body carries the policy
    /// as `healing_policy` (an opaque-body record) or
    /// `semantic.healing_policy` (an `environment_record`) — anything
    /// else is the typed refusal, never a substituted default.
    fn healing_policy_for(
        &self,
        run_id: &str,
        params: &Json,
    ) -> Result<hh_env::recovery::HealingPolicy, EmbedError> {
        if let Some(j) = params.get("policy") {
            return hh_env::recovery::HealingPolicy::from_json(j).ok_or_else(|| {
                EmbedError::SchemaViolation {
                    path: "heal/policy".into(),
                    code: "invalid_policy".into(),
                }
            });
        }
        let declared_ref = params
            .get("policy_ref")
            .and_then(Json::as_str)
            .map(str::to_string)
            .or_else(|| {
                self.store
                    .manifest(run_id)
                    .ok()
                    .and_then(|m| m.healing_policy_ref.clone())
            });
        if let Some(version_id) = declared_ref {
            let Some((_env, record)) = self.registry.get(&version_id) else {
                return Err(EmbedError::Refused {
                    reason: format!("healing_policy_ref_unresolvable:{version_id}"),
                });
            };
            let body = hh_registry::schema::body_json(record, true);
            let member = body
                .get("healing_policy")
                .or_else(|| body.get("semantic").and_then(|s| s.get("healing_policy")));
            let Some(j) = member else {
                return Err(EmbedError::Refused {
                    reason: format!("healing_policy_ref_no_policy:{version_id}"),
                });
            };
            return hh_env::recovery::HealingPolicy::from_json(j).ok_or_else(|| {
                EmbedError::Refused {
                    reason: format!("healing_policy_ref_malformed:{version_id}"),
                }
            });
        }
        let interactive = self
            .store
            .manifest(run_id)
            .map(|m| m.attendance.0 == hh_ledger::manifest::AttendanceValue::Interactive)
            .unwrap_or(false);
        Ok(hh_env::recovery::HealingPolicy {
            on_lost: if interactive {
                hh_env::recovery::OnLost::Ask
            } else {
                hh_env::recovery::OnLost::Reprovision
            },
            max_heals: 3,
            verify_after: None,
            preserve_detached: false,
        })
    }
}

// ── R2.3 unit battery — the legs the public boundary cannot produce between
// calls (an open `committed` effect, a `control.retry.scheduled` row, a
// `deliver_after`-withheld fire, a registry-resident `healing_policy` body).
// These drive the same handler code over a direct store mint — the rows land
// exactly as the producing ops would have written them.
#[cfg(test)]
mod r2_3_tests {
    use super::*;
    use crate::service::ServiceConfig;
    use hh_ledger::event::{Event, Producer, Scope};
    use hh_ledger::ids::ROOT_EVENT;
    use hh_wire::jsonrpc::Request;

    const TS: &str = "2026-01-01T00:00:00.000Z";

    fn dir(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "hh-embed-r23u-{}-{tag}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    fn call(svc: &mut EmbedService, method: &str, params: Json) -> Json {
        svc.handle(&Request {
            id: Json::str(format!("t-{method}")),
            method: method.into(),
            params,
        })
    }

    fn ok(resp: &Json) -> Json {
        resp.get("result")
            .unwrap_or_else(|| panic!("expected result, got {}", resp.to_canonical_string()))
            .clone()
    }

    fn err_kind(resp: &Json) -> String {
        resp.get("error")
            .and_then(|e| e.get("data"))
            .and_then(|d| d.get("kind"))
            .and_then(Json::as_str)
            .unwrap_or_else(|| panic!("expected error, got {}", resp.to_canonical_string()))
            .to_string()
    }

    fn svc() -> EmbedService {
        let root = dir("svc");
        let mut s = EmbedService::open(ServiceConfig {
            store_root: root.join("store"),
            kernel_version_id: "hh-kernel/0.1.0".into(),
            workspace_root: root.join("ws"),
            holder: "r23".into(),
        })
        .unwrap();
        // `hello` — the boundary stays `NotInitialized` until the
        // handshake; `experimental` opens the R2.3 ops' tier gate.
        let r = call(
            &mut s,
            "hello",
            Json::obj([
                ("contract_major", Json::Int(1)),
                (
                    "client",
                    Json::obj([
                        ("name", Json::str("r23")),
                        ("version", Json::str("1")),
                        ("kind", Json::str("test")),
                    ]),
                ),
                (
                    "capabilities",
                    Json::obj([("experimental", Json::Bool(true))]),
                ),
            ]),
        );
        assert!(r.get("result").is_some(), "hello: {r:?}");
        s
    }

    /// A writer session on a real run — `open_run` takes the writer
    /// lease; the session-table entry is the row
    /// `writer_session`/`env_subject` actually read (the durable state
    /// is identical either way — run + lease live in the store).
    fn writer(s: &mut EmbedService) -> (String, String) {
        writer_manifest(
            s,
            hh_ledger::manifest::RunManifest::minimal(hh_ledger::manifest::RunKind::Agent),
        )
    }

    fn writer_manifest(
        s: &mut EmbedService,
        manifest: hh_ledger::manifest::RunManifest,
    ) -> (String, String) {
        let (run_id, lease) = s.store.open_run(manifest, "r23-test").unwrap();
        let session_id = format!("sess-{run_id}-{}", uuidish());
        let head = s.store.head(&run_id).unwrap();
        s.sessions.insert(
            session_id.clone(),
            crate::service::SessionState {
                run_id: run_id.clone(),
                attach: false,
                lease: Some(lease),
                manifest_ref: "manifest:test".into(),
                realized: hh_embed_schema::types::RealizedSettings {
                    model_role_table_realized: Json::Null,
                    cwd: String::new(),
                    containment_effective: Json::Null,
                    policy_mode: "sync".into(),
                    profile_bindings: Json::Null,
                    protocol_bindings: vec![],
                    secrets_declared: vec![],
                    attendance: hh_embed_schema::types::AttendanceDeclaration {
                        value: "async".into(),
                        source: "declared".into(),
                    },
                },
                driver: None,
                mem_ctx: None,
                steering: (
                    hh_control::strategy::SteerMode::QueueNextTurn,
                    hh_control::strategy::ConcurrentInput::QueueOnly,
                ),
                env_json: Json::Null,
                env_handle_id: None,
                host_caps: vec![],
                turn_active: false,
                active_turn: String::new(),
                finished: false,
                detached: None,
                authority_caps: vec![],
                narrowing_leaf_ids: vec![],
                pendings: Default::default(),
                decided: Default::default(),
                delivered_wokens: Default::default(),
                host_asks: Default::default(),
                idem: Default::default(),
                budget_ceiling: Default::default(),
                leaf_arm: Default::default(),
                scan_seq: head.seq,
                next_invoke: None,
                next_completion: String::new(),
                next_response_ref: String::new(),
                model_fail_plan: Default::default(),
                client: None,
                contract_json: None,
                sink_seq: head.seq as i64,
            },
        );
        (session_id, run_id)
    }

    fn lease_of(s: &EmbedService, sid: &str) -> Lease {
        s.sessions
            .get(sid)
            .and_then(|sess| sess.lease.clone())
            .unwrap()
    }

    fn ev(class: &str, scope: Option<&str>, payload: Json) -> Event {
        Event {
            event_id: format!("{}-{}", class.replace('.', "-"), uuidish()),
            class: class.to_string(),
            ts: TS.to_string(),
            hlc: None,
            producer: Producer::kernel("kernel:test"),
            scope: Scope {
                effect_id: scope.map(str::to_string),
                ..Scope::default()
            },
            parent_event_id: ROOT_EVENT.to_string(),
            causes: Vec::new(),
            refs: Vec::new(),
            ir_refs: Vec::new(),
            surface_ids: Default::default(),
            provenance: Some(hh_provenance::ProvenanceRecord::kernel("kernel:test", 0)),
            content_kind: None,
            payload,
        }
    }

    fn uuidish() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(10_000);
        N.fetch_add(1, Ordering::SeqCst)
    }

    fn risk(rev: &str, rs: &str, scope: &str) -> Json {
        Json::obj([
            ("reversibility", Json::str(rev)),
            ("repeat_safety", Json::str(rs)),
            ("scope", Json::str(scope)),
        ])
    }

    /// `intended → authorized → decided → prepared → committed [→
    /// observed{applied}]` — the ledger-legal chain (mirrors
    /// hh-ledger/tests/branch_model.rs `to_applied`).
    fn apply_effect(
        s: &mut EmbedService,
        run: &str,
        lease: &Lease,
        eid: &str,
        terminal: bool,
        compensable: bool,
    ) {
        let rc = if compensable {
            risk("compensable", "idempotent", "workspace_local")
        } else {
            risk("irreversible", "non_idempotent", "external")
        };
        let mut prep = Json::obj([("idempotency_key", Json::str(format!("key-{eid}")))]);
        if compensable {
            if let Json::Obj(m) = &mut prep {
                m.insert(
                    "compensation_plan_id".into(),
                    Json::str(format!("plan-{eid}")),
                );
            }
        }
        let mut batch = vec![
            ev(
                "action.effect.intended",
                Some(eid),
                Json::obj([
                    ("effect_id", Json::str(eid)),
                    ("effective_risk_class", rc.clone()),
                ]),
            ),
            ev(
                "action.effect.authorized",
                Some(eid),
                Json::obj([("effective_risk_class", rc)]),
            ),
            ev(
                "security.permission.decided",
                Some(eid),
                Json::obj([
                    ("effect_id", Json::str(eid)),
                    ("attempt_no", Json::Int(1)),
                    ("decision", Json::str("allow")),
                ]),
            ),
            ev("action.effect.prepared", Some(eid), prep),
            ev(
                "action.effect.committed",
                Some(eid),
                Json::obj([
                    ("attempt_no", Json::Int(1)),
                    ("fencing_token", Json::Int(lease.generation as i64)),
                ]),
            ),
        ];
        if terminal {
            batch.push(ev(
                "action.effect.observed",
                Some(eid),
                Json::obj([
                    ("attempt_no", Json::Int(1)),
                    ("outcome", Json::str("applied")),
                    ("fencing_token", Json::Int(lease.generation as i64)),
                ]),
            ));
        }
        s.store.append(run, lease, batch).unwrap();
    }

    fn observe(s: &mut EmbedService, run: &str, lease: &Lease, eid: &str) {
        s.store
            .append(
                run,
                lease,
                vec![ev(
                    "action.effect.observed",
                    Some(eid),
                    Json::obj([
                        ("attempt_no", Json::Int(1)),
                        ("outcome", Json::str("applied")),
                        ("fencing_token", Json::Int(lease.generation as i64)),
                    ]),
                )],
            )
            .unwrap();
    }

    fn seqs_of(s: &EmbedService, run: &str) -> Vec<(u64, String)> {
        s.store()
            .envelopes(run)
            .unwrap()
            .iter()
            .map(|e| (e.seq, e.class.clone()))
            .collect()
    }

    /// S-1 at the boundary: a `committed` effect refuses `suspend` typed
    /// `OpenCommittedEffects` *before* any producer row lands — the
    /// ledger carries no `wakeup.scheduled` residue and no `suspended`.
    #[test]
    fn r2_3_suspend_s1_refuses_open_committed_effect() {
        let mut s = svc();
        let (sid, run) = writer(&mut s);
        let lease = lease_of(&s, &sid);
        apply_effect(&mut s, &run, &lease, "eff-1", false, false);

        let r = call(
            &mut s,
            "suspend",
            Json::obj([
                ("session_id", Json::str(sid.clone())),
                (
                    "reasons",
                    Json::Arr(vec![Json::obj([("type", Json::str("operator_pause"))])]),
                ),
            ]),
        );
        assert_eq!(err_kind(&r), "Refused", "{r:?}");
        let reason = r
            .get("error")
            .and_then(|e| e.get("data"))
            .and_then(|d| d.get("reason"))
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        assert!(reason.contains("OpenCommittedEffects"), "{reason}");
        // No residue — the pre-check runs before any producing mint.
        assert!(seqs_of(&s, &run)
            .iter()
            .all(|(_, c)| c != "lifecycle.run.suspended" && c != "control.wakeup.scheduled"));
        // The terminal fold releases S-1 — the same run suspends once
        // the effect lands `observed`.
        observe(&mut s, &run, &lease, "eff-1");
        let out = ok(&call(
            &mut s,
            "suspend",
            Json::obj([
                ("session_id", Json::str(sid)),
                (
                    "reasons",
                    Json::Arr(vec![Json::obj([("type", Json::str("operator_pause"))])]),
                ),
            ]),
        ));
        assert_eq!(out.get("released_lease"), Some(&Json::Bool(true)));
    }

    /// `compensate` walks the applied+compensable set — a declared
    /// outcome lands `action.effect.compensated`; a missing outcome is
    /// the honest `abandoned` + `lifecycle.escalation.raised` leg, and a
    /// replay never re-dispatches.
    #[test]
    fn r2_3_compensate_walks_and_escalates_honestly() {
        let mut s = svc();
        let (sid, run) = writer(&mut s);
        let lease = lease_of(&s, &sid);
        apply_effect(&mut s, &run, &lease, "eff-1", true, true);
        apply_effect(&mut s, &run, &lease, "eff-2", true, true);

        let out = ok(&call(
            &mut s,
            "compensate",
            Json::obj([
                ("session_id", Json::str(sid.clone())),
                (
                    "outcomes",
                    Json::obj([("eff-1", Json::obj([("outcome", Json::str("applied"))]))]),
                ),
            ]),
        ));
        let names = |k: &str| -> Vec<String> {
            out.get(k)
                .and_then(|v| match v {
                    Json::Arr(a) => Some(
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect(),
                    ),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(names("compensated"), vec!["eff-1".to_string()], "{out:?}");
        assert_eq!(names("abandoned"), vec!["eff-2".to_string()], "{out:?}");
        assert_eq!(names("escalations").len(), 1, "{out:?}");
        // The rows are the record: the compensating lifecycle + the
        // escalation — durable before the result returned.
        let classes: Vec<String> = seqs_of(&s, &run).iter().map(|(_, c)| c.clone()).collect();
        assert!(classes.iter().any(|c| c == "action.effect.compensated"));
        assert!(classes.iter().any(|c| c == "lifecycle.escalation.raised"));
        // The compensating effect exists as its own lifecycle scope.
        let folds = s.store.effect_folds(&run).unwrap();
        assert!(folds.iter().any(|(eid, _)| eid == "eff-1~comp"));

        // Replay — the terminal compensator is never re-dispatched; the
        // report is stable.
        let out2 = ok(&call(
            &mut s,
            "compensate",
            Json::obj([("session_id", Json::str(sid.clone()))]),
        ));
        assert!(out2.get("compensated").is_some(), "{out2:?}");
    }

    /// The `retry_due` producer leg (DF-S2.3-1): a durable
    /// `control.retry.scheduled` row mints its `retry_due` subscription
    /// inside `deliver_wakeup` — `control.wakeup.scheduled` is durable
    /// before the `occurred`/`fired` pair it synthesizes, and the drain
    /// yields `RetryDue{scope_id}`.
    #[test]
    fn r2_3_retry_scheduled_produces_retry_due_subscription() {
        let mut s = svc();
        let (sid, run) = writer(&mut s);
        let lease = lease_of(&s, &sid);
        s.store
            .append(
                &run,
                &lease,
                vec![ev(
                    "control.retry.scheduled",
                    None,
                    Json::obj([
                        ("scope_id", Json::str("mc-1")),
                        ("kind", Json::str("model_call")),
                        ("not_before", Json::Int(0)),
                        ("attempt_no", Json::Int(2)),
                        ("reason", Json::str("transient")),
                    ]),
                )],
            )
            .unwrap();
        let now = s.store.now_ms();
        s.store.deliver_wakeup(&run, &lease, now).unwrap();
        // The subscription minted (one per scope).
        let subs = s.store.wakeup_subscriptions(&run).unwrap();
        assert!(subs.iter().any(|w| matches!(
            &w.trigger,
            Trigger::RetryDue { scope_id } if scope_id == "mc-1"
        )));
        // Durable-before-visible — `occurred` precedes `fired`.
        let evs = seqs_of(&s, &run);
        let occurred = evs
            .iter()
            .rfind(|(_, c)| c == "control.wakeup.occurred")
            .map(|(x, _)| *x);
        let fired = evs
            .iter()
            .rfind(|(_, c)| c == "control.wakeup.fired")
            .map(|(x, _)| *x);
        assert!(occurred.is_some() && fired.is_some() && occurred < fired);
        // The drain yields the RetryDue woken — the run's re-cue.
        let drained = s.store.wakeup_drain(&run).unwrap();
        assert!(drained.iter().any(|w| matches!(
            &w.trigger,
            Trigger::RetryDue { scope_id } if scope_id == "mc-1"
        )));
        // Idempotent — a second pass neither re-mints the subscription
        // nor re-fires the consumed occurrence key.
        let n_subs = s.store.wakeup_subscriptions(&run).unwrap().len();
        s.store.deliver_wakeup(&run, &lease, now + 1).unwrap();
        assert_eq!(s.store.wakeup_subscriptions(&run).unwrap().len(), n_subs);
    }

    /// The `effect_terminal` producer leg: a `fired` occurrence withheld
    /// by `deliver_after` mints the `effect_terminal` subscription that
    /// re-cues the run when the blocking effect folds terminal — the
    /// withheld delivery lands with the re-cue, never silently.
    #[test]
    fn r2_3_deliver_after_blocked_fire_produces_effect_terminal_sub() {
        let mut s = svc();
        let (sid, run) = writer(&mut s);
        let lease = lease_of(&s, &sid);
        apply_effect(&mut s, &run, &lease, "eff-1", false, false);
        // A `follow_up` subscription whose fire withholds on the
        // committed effect (W-3).
        let created_by = hh_ledger::manifest::EventRef {
            run_id: run.clone(),
            event_id: s.store.head(&run).unwrap().event_id,
        };
        let sub = s
            .store
            .wakeup_subscribe(
                &run,
                &lease,
                Trigger::External {
                    kind: "webhook".into(),
                    source_ref: Some("ing:test".into()),
                    filter: None,
                },
                WakeupPolicy::default_policy(),
                &created_by,
            )
            .unwrap();
        let now = s.store.now_ms();
        s.store
            .wakeup_occurred(&run, &lease, &sub, "k-1", None, now)
            .unwrap();
        s.store.deliver_wakeup(&run, &lease, now).unwrap();
        // Blocked — the fired occurrence waits on `eff-1`.
        assert!(s.store.wakeup_drain(&run).unwrap().is_empty());
        // The producer pass reads durable state — the withheld fire
        // lands its `effect_terminal` subscription on the next pass.
        s.store.deliver_wakeup(&run, &lease, now).unwrap();
        let subs = s.store.wakeup_subscriptions(&run).unwrap();
        assert!(subs.iter().any(|w| matches!(
            &w.trigger,
            Trigger::EffectTerminal { effect_id } if effect_id == "eff-1"
        )));
        // Fold the effect terminal — the next pass synthesizes
        // `effect:eff-1`, fires it, and the withheld delivery unblocks.
        observe(&mut s, &run, &lease, "eff-1");
        s.store.deliver_wakeup(&run, &lease, now + 1).unwrap();
        let drained = s.store.wakeup_drain(&run).unwrap();
        assert!(
            drained.len() >= 2,
            "withheld + re-cue deliveries: {drained:?}"
        );
        assert!(drained.iter().any(|w| matches!(
            &w.trigger,
            Trigger::EffectTerminal { effect_id } if effect_id == "eff-1"
        )));
    }

    /// `healing_policy_ref` resolves through the one `RegistryStore` —
    /// the record's `semantic.healing_policy` is the policy; a record
    /// with no policy member and an unregistered ref are both typed
    /// refusals, never a substituted default.
    #[test]
    fn r2_3_heal_policy_ref_resolution() {
        let mut s = svc();
        let (sid, run) = writer(&mut s);
        arm_lost_env(&mut s, &run, &sid);
        let prov = hh_provenance::ProvenanceRecord::kernel("r23-test", 0);
        let env_body = Json::obj([
            ("kind", Json::str("environment")),
            (
                "semantic",
                Json::obj([
                    (
                        "healing_policy",
                        Json::obj([("on_lost", Json::str("ask")), ("max_heals", Json::Int(3))]),
                    ),
                    (
                        "containment_policy",
                        Json::obj([("version_id", Json::str("cp:test"))]),
                    ),
                ]),
            ),
            ("surface", Json::obj([])),
            ("refs", Json::Arr(vec![])),
        ]);
        let vref = s
            .registry
            .register(
                hh_registry::records::RegistryRecord::EnvironmentRecord(env_body),
                &prov,
                None,
            )
            .unwrap();
        let vid = vref.version_id.clone();
        // `policy_ref` param — resolves to the `ask` ladder (the
        // attendance default would `reprovision` → the `fail_run`
        // posture's `replace` would error; the divergence is the proof
        // the registry record was read).
        let out = ok(&call(
            &mut s,
            "heal",
            Json::obj([
                ("session_id", Json::str(sid.clone())),
                ("policy_ref", Json::str(vid.clone())),
            ]),
        ));
        assert_eq!(
            out.get("outcome").and_then(Json::as_str),
            Some("ask"),
            "{out:?}"
        );
        assert!(out.get("permission_id").and_then(Json::as_str).is_some());
        // A registered record with no `healing_policy` member — the
        // typed no-policy refusal.
        let empty_body = Json::obj([
            ("kind", Json::str("environment")),
            (
                "semantic",
                Json::obj([(
                    "containment_policy",
                    Json::obj([("version_id", Json::str("cp:test"))]),
                )]),
            ),
            ("surface", Json::obj([])),
            ("refs", Json::Arr(vec![])),
        ]);
        let v2 = s
            .registry
            .register(
                hh_registry::records::RegistryRecord::EnvironmentRecord(empty_body),
                &prov,
                None,
            )
            .unwrap();
        assert_eq!(
            err_kind(&call(
                &mut s,
                "heal",
                Json::obj([
                    ("session_id", Json::str(sid.clone())),
                    ("policy_ref", Json::str(v2.version_id.clone())),
                ]),
            )),
            "Refused"
        );
        // An unregistered ref — the same typed refusal family.
        assert_eq!(
            err_kind(&call(
                &mut s,
                "heal",
                Json::obj([
                    ("session_id", Json::str(sid.clone())),
                    ("policy_ref", Json::str("hh/missing:nope")),
                ]),
            )),
            "Refused"
        );
        // The manifest member — `connection_info{healing_policy_ref}`
        // declares it at open: a run minted with the ref in its
        // manifest resolves the same record when params carry nothing.
        let (sid2, run2) = {
            let mut m =
                hh_ledger::manifest::RunManifest::minimal(hh_ledger::manifest::RunKind::Agent);
            m.healing_policy_ref = Some(vid.clone());
            writer_manifest(&mut s, m)
        };
        arm_lost_env(&mut s, &run2, &sid2);
        let out2 = ok(&call(
            &mut s,
            "heal",
            Json::obj([("session_id", Json::str(sid2))]),
        ));
        assert_eq!(
            out2.get("outcome").and_then(Json::as_str),
            Some("ask"),
            "{out2:?}"
        );
    }

    /// Arm a `local_host` env driver over a workspace that does not
    /// exist — `verify_environment_verdict` reads
    /// `lost{workspace_missing}` (the same verdict a dead workspace
    /// produces on the real open path).
    fn arm_lost_env(s: &mut EmbedService, run: &str, sid: &str) {
        use hh_env::handle::{OnLoss, Roots};
        use hh_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
        let ws = s.store.root().join("missing-ws").display().to_string();
        let record = EnvironmentRecord {
            class: EnvironmentClass::LocalHost,
            image: ImageRef::ContentAddress(hh_identity::address(
                b"hh-embed/local_host:test",
                "application/vnd.hh.env",
            )),
            build_context: None,
            platform: "test".into(),
            provisioning: ProvisioningRecipe::default(),
            containment_policy_ref: ("k".into(), "v1".into()),
            limits: Default::default(),
            nondeterminism: vec![],
            unpinned: Default::default(),
            ext: Default::default(),
            image_attestation: None,
            canary_channels: vec![],
        };
        let roots = Roots {
            workspace_roots: vec![ws.clone()],
            writable_roots: vec![ws.clone()],
            cwd: ws,
        };
        let lease = lease_of(s, sid);
        let driver = s
            .env_drivers
            .entry(run.to_string())
            .or_insert_with(|| hh_env::driver::EnvDriver::new(run));
        let handle = driver
            .provision(
                &mut s.store,
                &lease,
                &record,
                roots,
                hh_containment::attach::PolicySlot::Inline(Box::new(
                    hh_containment::policy::kernel_default(0),
                )),
                OnLoss::FailRun,
                Some(&mut s.credential_broker),
            )
            .unwrap();
        s.sessions.get_mut(sid).unwrap().env_handle_id = Some(handle.env_handle_id.clone());
    }
}
