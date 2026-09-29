//! The `R-2.2.3⁰ᵇ`/C1 environment-recovery surface (§5a.3; ADR-0132; S2.3):
//!
//! * [`EnvDriver::verify_environment_verdict`] — the level-triggered resume
//!   check: `EnvironmentVerdict ∈ {attached, reattachable, lost{cause}}`,
//!   computed from the live session + filesystem + containment report — never
//!   the stale in-memory handle — and recorded as `action.environment.verified`.
//! * [`EnvDriver::heal_with_policy`] — `heal(run, lease, handle,
//!   policy: HealingPolicy)`: policy-bound healing (`on_lost`, `max_heals`
//!   counted from the ledger, `lost_items_ref`, `action.environment.healed` —
//!   audit-grade).
//! * [`EnvDriver::reconcile_detached`] — the `preserve_until` reconciliation:
//!   every detached child the helper preserved is reconciled to
//!   `observed | unknown | terminated` by the resuming writer
//!   (AC-R-2.2.3-14).

use std::collections::BTreeMap;

use hh_containment::report::verify_report;
use hh_helper::protocol::HelperRequest;
use hh_ledger::store::{Lease, Store};
use hh_wire::json::Json;

use crate::driver::EnvDriver;
use crate::errors::EnvError;
use crate::events::{self, EventMinter, ScopeChain};
use crate::handle::HandleState;
use crate::observe;

/// `lost{cause}` — the closed cause set (§5a.3 `verify_environment`'s
/// `lost{cause ∈ {helper_gone, image_missing, workspace_missing, lease_expired,
/// containment_changed}}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LostCause {
    /// The helper process is gone and no session answers.
    HelperGone,
    /// The declared image no longer resolves.
    ImageMissing,
    /// A workspace root is gone from the filesystem.
    WorkspaceMissing,
    /// The environment lease lapsed unrecoverably.
    LeaseExpired,
    /// The `ContainmentReport` in force no longer matches the manifest's.
    ContainmentChanged,
}

impl LostCause {
    /// The closed-set spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            LostCause::HelperGone => "helper_gone",
            LostCause::ImageMissing => "image_missing",
            LostCause::WorkspaceMissing => "workspace_missing",
            LostCause::LeaseExpired => "lease_expired",
            LostCause::ContainmentChanged => "containment_changed",
        }
    }
}

/// `EnvironmentVerdict ∈ {attached, reattachable, lost{cause}}` (§5a.3;
/// ADR-0132 §1) — computed fresh, never trusted from memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvironmentVerdict {
    /// The session answers and the containment report is in force.
    Attached {
        /// The live helper session id.
        helper_session: String,
    },
    /// The session answers but the handle is `unreachable` — `heal`'s
    /// reattach rung applies.
    Reattachable {
        /// The live helper session id.
        helper_session: String,
    },
    /// The environment is gone — `heal_with_policy` owns the decision.
    Lost {
        /// Why.
        cause: LostCause,
    },
}

impl EnvironmentVerdict {
    /// The `verdict` member spelling for `action.environment.verified`.
    pub fn to_json(&self) -> Json {
        match self {
            EnvironmentVerdict::Attached { helper_session } => Json::obj([
                ("verdict", Json::str("attached")),
                ("helper_session", Json::str(helper_session)),
            ]),
            EnvironmentVerdict::Reattachable { helper_session } => Json::obj([
                ("verdict", Json::str("reattachable")),
                ("helper_session", Json::str(helper_session)),
            ]),
            EnvironmentVerdict::Lost { cause } => Json::obj([
                ("verdict", Json::str("lost")),
                ("cause", Json::str(cause.as_str())),
            ]),
        }
    }

    /// `lost{..}`?
    pub fn is_lost(&self) -> bool {
        matches!(self, EnvironmentVerdict::Lost { .. })
    }
}

/// `HealingPolicy{on_lost, max_heals, verify_after?, preserve_detached}` — the
/// MUST-data artifact `healing_policy_ref` names (§5a.3; ADR-0132 §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealingPolicy {
    /// What a `lost{cause}` verdict does.
    pub on_lost: OnLost,
    /// The heal bound — `heal_no > max_heals` ⇒ `MaxHealsExceeded` ⇒ the
    /// driver's `stop{infrastructure_failure{environment_lost}}`.
    pub max_heals: u32,
    /// Re-verify the healed handle through this validator before `ready`
    /// (optional).
    pub verify_after: Option<String>,
    /// Keep preserved detached children through the heal (`preserve_until`).
    pub preserve_detached: bool,
}

/// `on_lost ∈ {reprovision, snapshot_restore_then_reprovision, ask, fail}`
/// (ADR-0132 §2; defaults follow `attendance`: `interactive → ask`,
/// `async|unattended → reprovision`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnLost {
    /// Reprovision from `environment_ref` through the effect lifecycle.
    Reprovision,
    /// Restore the latest snapshot, then reprovision what it didn't cover.
    SnapshotRestoreThenReprovision,
    /// Raise an `ask` (the interactive-attendance default).
    Ask,
    /// Refuse — the run stops `environment_lost`.
    Fail,
}

/// What `heal_with_policy` did — `healed{…} | refused{reason} |
/// ask{permission_id}` (§5a.3's return sum).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealOutcome {
    /// The heal landed.
    Healed {
        /// The successor handle id (== `from` on a reattach).
        new_handle: String,
        /// `full | snapshot{ref} | none`.
        restored: String,
        /// The blob address of the lost-items observation (`authority =
        /// environment` — the driver's next context carries it).
        lost_items_ref: String,
        /// The ledger-counted heal number.
        heal_no: u32,
    },
    /// `on_lost = ask` — the pending permission is the durable record.
    Ask {
        /// The minted `security.permission.pending` id.
        permission_id: String,
    },
    /// `on_lost = fail` (or a policy denial) — the caller stops the run
    /// `infrastructure_failure{environment_lost}`.
    Refused {
        /// Why.
        reason: String,
    },
}

/// One detached child's reconciliation (AC-R-2.2.3-14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachedReconciliation {
    /// The detached child's effect id.
    pub effect_id: String,
    /// `observed | unknown | terminated`.
    pub verdict: &'static str,
    /// The terminal/transition row this reconciliation wrote, if any.
    pub event_id: Option<String>,
}

impl EnvDriver {
    /// `verify_environment(run, lease, handle)` — the level-triggered verdict
    /// (§5a.3; mandatory for every handle in the checkpoint view before
    /// `Cue.resumed`). Computes from the live channel + filesystem + the
    /// containment report — never the stale handle — and appends
    /// `action.environment.verified{env_handle, verdict}`.
    pub fn verify_environment_verdict(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
    ) -> Result<EnvironmentVerdict, EnvError> {
        let h = self
            .handles
            .get(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?
            .clone();
        // A missing workspace root is `lost{workspace_missing}` regardless of
        // channel state (the helper may outlive its workspace).
        let workspace_missing = h
            .roots
            .workspace_roots
            .iter()
            .any(|r| !std::path::Path::new(r).exists());
        // `local_host` holds no helper channel — its `EnvSession` is the
        // in-process executor, live iff the handle still holds it (the
        // kernel *is* the helper; a dead kernel verifies nothing). Helper
        // classes probe the live `HelperClient`.
        let session_live = self
            .sessions
            .get_mut(env_handle_id)
            .map(|c| c.is_live())
            .unwrap_or_else(|| {
                h.class == crate::record::EnvironmentClass::LocalHost && h.session.is_some()
            });
        let session_id = self
            .sessions
            .get(env_handle_id)
            .map(|c| c.session_id.clone())
            .or_else(|| h.session.as_ref().map(|s| s.session_id.clone()))
            .unwrap_or_default();
        let containment_fresh = h
            .report
            .as_ref()
            .map(|r| verify_report(r, &h.containment.policy().version_id).is_ok())
            .unwrap_or(false);
        let verdict = if workspace_missing {
            EnvironmentVerdict::Lost {
                cause: LostCause::WorkspaceMissing,
            }
        } else if session_live && h.state == HandleState::Ready && containment_fresh {
            EnvironmentVerdict::Attached {
                helper_session: session_id,
            }
        } else if session_live
            && matches!(h.state, HandleState::Unreachable | HandleState::Detached)
        {
            EnvironmentVerdict::Reattachable {
                helper_session: session_id,
            }
        } else if session_live && !containment_fresh {
            EnvironmentVerdict::Lost {
                cause: LostCause::ContainmentChanged,
            }
        } else {
            EnvironmentVerdict::Lost {
                cause: LostCause::HelperGone,
            }
        };
        let ev = EventMinter::new(store, &self.run_id).mint(
            "action.environment.verified",
            Json::obj([
                ("env_handle", Json::str(env_handle_id)),
                ("handle_state", Json::str(h.state.as_str())),
                ("verdict", verdict.to_json()),
            ]),
        )?;
        store.append(&self.run_id, lease, vec![ev])?;
        Ok(verdict)
    }

    /// `heal(run, lease, handle, policy)` — the policy-bound heal (§5a.3;
    /// ADR-0132 §2). `heal_no` is counted from the ledger's
    /// `action.environment.healed` rows for this handle — never from process
    /// memory. `heal_no >= max_heals` ⇒ `MaxHealsExceeded`.
    pub fn heal_with_policy(
        &mut self,
        #[cfg_attr(not(feature = "tier-c1"), allow(unused_variables))] store: &mut Store,
        #[cfg_attr(not(feature = "tier-c1"), allow(unused_variables))] lease: &Lease,
        #[cfg_attr(not(feature = "tier-c1"), allow(unused_variables))] env_handle_id: &str,
        #[cfg_attr(not(feature = "tier-c1"), allow(unused_variables))] policy: &HealingPolicy,
        #[cfg_attr(not(feature = "tier-c1"), allow(unused_variables))] cause: &str,
    ) -> Result<HealOutcome, EnvError> {
        // CC6 removability — healing is the C1 surface (§5a.3); a build
        // without `tier-c1` refuses typed `Unsupported`, never degrades.
        #[cfg(not(feature = "tier-c1"))]
        return Err(EnvError::Unsupported {
            capability: "heal",
            detail: "tier-c1 not built (HealingPolicy healing is the C1 surface)".to_string(),
        });
        #[cfg(feature = "tier-c1")]
        {
            // heal_no — counted from the durable rows (a restart recounts the
            // same number; the bound can't be bypassed by losing memory).
            let heal_no = store
                .events(&self.run_id)?
                .iter()
                .filter(|e| {
                    e.class == "action.environment.healed"
                        && e.payload.get("from_handle").and_then(Json::as_str)
                            == Some(env_handle_id)
                })
                .count() as u32;
            if heal_no >= policy.max_heals {
                return Err(EnvError::MaxHealsExceeded {
                    env_handle_id: env_handle_id.to_string(),
                    heal_no,
                    max_heals: policy.max_heals,
                });
            }
            match policy.on_lost {
                OnLost::Ask => {
                    // The durable ask — `security.permission.pending` is the
                    // owed-decision row (§5g.7; the ephemeral `requested`
                    // rendering is the surface's, not the record's).
                    let permission_id = store.alloc_id("perm");
                    let ev = EventMinter::new(store, &self.run_id).mint(
                        "security.permission.pending",
                        Json::obj([
                            ("permission_id", Json::str(&permission_id)),
                            (
                                "request",
                                Json::obj([
                                    ("subject_ref", Json::str(env_handle_id)),
                                    ("capability_ref", Json::str("environment.heal")),
                                    ("args_canonical_hash", Json::str("")),
                                    ("reason", Json::str(cause)),
                                ]),
                            ),
                            ("requested_at", Json::str(store.ts_now())),
                            ("mode", Json::str("once")),
                        ]),
                    )?;
                    store.append(&self.run_id, lease, vec![ev])?;
                    return Ok(HealOutcome::Ask { permission_id });
                }
                OnLost::Fail => {
                    return Ok(HealOutcome::Refused {
                        reason: "healing_policy.on_lost = fail".to_string(),
                    });
                }
                OnLost::Reprovision | OnLost::SnapshotRestoreThenReprovision => {}
            }

            // `lost_items_ref` — the observation payload BEFORE the heal
            // destroys state: uncommitted files (no post-snapshot `fs_tree`
            // covers them) and the detached children this reconciliation
            // records `terminated` (ADR-0132 §2 — delivered with
            // `authority = environment`, never silently).
            let lost_items = self.collect_lost_items(store, env_handle_id);
            let lost_items_ref = store
                .put_blob(
                    Json::obj([("lost_items", Json::Arr(lost_items))])
                        .to_canonical_string()
                        .as_bytes(),
                    "application/json",
                )
                .map_err(EnvError::Ledger)?;

            // The heal: reattach when the session still answers (kernel death ≠
            // environment death), else `replace` per `on_loss` — a reprovision.
            let h = self
                .handles
                .get(env_handle_id)
                .ok_or(EnvError::Unavailable {
                    env_handle_id: env_handle_id.to_string(),
                    state: "missing",
                })?
                .clone();
            let session_live = self
                .sessions
                .get_mut(env_handle_id)
                .map(|c| c.is_live())
                .unwrap_or(false);
            let (new_handle, restored) = if session_live && h.state == HandleState::Unreachable {
                self.heal(store, lease, env_handle_id, true)?;
                (env_handle_id.to_string(), "full".to_string())
            } else {
                let succ = self.replace(store, lease, env_handle_id)?;
                let restored = if policy.on_lost == OnLost::SnapshotRestoreThenReprovision
                    && !h.snapshots.is_empty()
                {
                    format!(
                        "snapshot:{}",
                        h.snapshots.last().cloned().unwrap_or_default()
                    )
                } else {
                    "none".to_string()
                };
                (succ.env_handle_id.clone(), restored)
            };
            // `action.environment.healed{from_handle, to_handle, cause, restored,
            // heal_no, verification_verdict}` — audit-grade (§5g.6 §3).
            let verdict = self
                .verify_environment_verdict(store, lease, &new_handle)
                .map(|v| v.to_json())
                .unwrap_or(Json::Null);
            let ev = EventMinter::new(store, &self.run_id).mint(
                "action.environment.healed",
                Json::obj([
                    ("from_handle", Json::str(env_handle_id)),
                    ("to_handle", Json::str(&new_handle)),
                    ("cause", Json::str(cause)),
                    ("restored", Json::str(&restored)),
                    ("heal_no", Json::Int((heal_no + 1) as i64)),
                    ("verification_verdict", verdict),
                    ("lost_items_ref", Json::str(lost_items_ref.id())),
                ]),
            )?;
            store.append(&self.run_id, lease, vec![ev])?;
            Ok(HealOutcome::Healed {
                new_handle,
                restored,
                lost_items_ref: lost_items_ref.id(),
                heal_no: heal_no + 1,
            })
        }
    }

    /// The lost-items observation — the detached children a lost environment
    /// terminates plus unsnapshotted workspace roots (names only — content
    /// never enters the row; the blob is the observation's body).
    #[cfg(feature = "tier-c1")]
    fn collect_lost_items(&self, store: &Store, env_handle_id: &str) -> Vec<Json> {
        let mut out = Vec::new();
        if let Ok(folds) = store.effect_folds(&self.run_id) {
            for (eid, f) in &folds {
                if !f.is_terminal() {
                    out.push(Json::obj([
                        ("kind", Json::str("detached_child")),
                        ("effect_id", Json::str(eid)),
                        ("detail", Json::str("terminated by environment loss")),
                    ]));
                }
            }
        }
        if let Some(h) = self.handles.get(env_handle_id) {
            if h.snapshots.is_empty() {
                for root in &h.roots.workspace_roots {
                    out.push(Json::obj([
                        ("kind", Json::str("unsnapshotted_state")),
                        ("path", Json::str(root)),
                    ]));
                }
            }
        }
        out
    }

    /// `reconcile_detached(env_handle_id)` — the `preserve_until` sweep
    /// (AC-R-2.2.3-14; ADR-0132 §3): `list_detached` reports every surviving
    /// detached process group; each is reconciled to `observed | unknown |
    /// terminated` with the terminal written by this writer. A child absent
    /// from the list (past `preserve_until` — the helper reaped it) is
    /// `terminated`.
    pub fn reconcile_detached(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
    ) -> Result<Vec<DetachedReconciliation>, EnvError> {
        // `list_detached` — the (execution_id, effect_id) pairs the helper
        // still holds alive (the `effect_id` member is additive — CC8).
        let mut alive: Vec<(String, String)> = Vec::new();
        if let Some(client) = self.sessions.get_mut(env_handle_id) {
            if let Ok(resp) = client.request(&HelperRequest::ListDetached) {
                if let Some(Json::Arr(rows)) = resp.get("detached") {
                    for r in rows {
                        let execution_id = r
                            .get("execution_id")
                            .and_then(Json::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let effect_id = r
                            .get("effect_id")
                            .and_then(Json::as_str)
                            .unwrap_or_default()
                            .to_string();
                        alive.push((execution_id, effect_id));
                    }
                }
            }
        }
        // Every detached-child effect the ledger knows — the union of
        // `detached_effect_ids[]` members and the `list_detached` effects.
        let mut detached: BTreeMap<String, String> = BTreeMap::new(); // effect_id → execution_id
        for e in store.events(&self.run_id)? {
            if e.class == "action.effect.committed" {
                if let Some(Json::Arr(ids)) = e.payload.get("detached_effect_ids") {
                    for id in ids.iter().filter_map(Json::as_str) {
                        detached.entry(id.to_string()).or_default();
                    }
                }
            }
        }
        for (execution_id, effect_id) in &alive {
            if !effect_id.is_empty() {
                detached.insert(effect_id.clone(), execution_id.clone());
            }
        }
        let gen = lease.generation;
        let mut out = Vec::new();
        for (effect_id, execution_id) in detached {
            let Some(f) = store.effect_fold(&self.run_id, &effect_id)? else {
                continue;
            };
            if f.is_terminal() {
                out.push(DetachedReconciliation {
                    effect_id,
                    verdict: "observed",
                    event_id: None,
                });
                continue;
            }
            let chain = ScopeChain {
                turn_id: f.turn_id.clone().unwrap_or_default(),
                model_call_id: f.model_call_id.clone().unwrap_or_default(),
                tool_call_id: f.tool_call_id.clone().unwrap_or_default(),
            };
            if execution_id.is_empty() {
                // Past `preserve_until` — the helper reaped the group; the
                // resuming writer records `terminated` (an `observed` whose
                // `terminated` member marks the reconciliation path).
                let obs = observe::Observation {
                    outcome: observe::EffectOutcome::NotApplied,
                    status: observe::ObservedStatus::Error {
                        class: observe::ErrorClass::EnvironmentUnavailable,
                        origin: observe::ErrorOrigin::Execution,
                        detail_ref: None,
                        retryable: false,
                    },
                    exit_status: None,
                    manifest_ref: String::new(),
                    completeness: crate::capture::Completeness::Unknown,
                    // The reconciliation path carries no admission — the
                    // terminated attempt's result never ran.
                    admission: None,
                };
                // `attempt_no` binds the committed attempt — the terminal
                // observes *that* attempt, never a phantom retry.
                let mut p = events::observed_payload(f.attempt_no, gen, &obs);
                if let Json::Obj(m) = &mut p {
                    m.insert("terminated".to_string(), Json::Bool(true));
                }
                let ev = EventMinter::new(store, &self.run_id).mint_effect(
                    "action.effect.observed",
                    p,
                    &effect_id,
                    &chain,
                )?;
                let id = ev.event_id.clone();
                store.append(&self.run_id, lease, vec![ev])?;
                out.push(DetachedReconciliation {
                    effect_id,
                    verdict: "terminated",
                    event_id: Some(id),
                });
            } else {
                // Still alive — `unknown{cause: detached_reconcile}`; the
                // probe policy owns the terminal.
                let ev = EventMinter::new(store, &self.run_id).mint_effect(
                    "action.effect.unknown",
                    events::unknown_payload(f.attempt_no, gen, "detached_reconcile"),
                    &effect_id,
                    &chain,
                )?;
                let id = ev.event_id.clone();
                store.append(&self.run_id, lease, vec![ev])?;
                out.push(DetachedReconciliation {
                    effect_id,
                    verdict: "unknown",
                    event_id: Some(id),
                });
            }
        }
        Ok(out)
    }
}
