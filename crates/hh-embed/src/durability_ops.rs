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
use hh_ledger::goal::ContinueCarried;
use hh_ledger::manifest::EventRef;
use hh_ledger::store::Lease;
use hh_ledger::wakeup::{Trigger, WakeupPolicy};
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
}
