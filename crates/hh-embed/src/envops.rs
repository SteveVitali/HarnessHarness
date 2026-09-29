//! The Group M environment ops (§7.1 `env *`; ADR-0136…0138; ADR-0177
//! D7's `instrument` charging) — `env.snapshot`, `env.derive`,
//! `env.set_phase` over the session's environment handle.
//!
//! Boundary rules these honor:
//! - **R-NOSIDE** — an `EnvHandleId` never leaves the kernel; results
//!   carry snapshot refs, modes and states, never handle identifiers.
//! - **Instrument charging** — a Group M `env.snapshot` records
//!   `taken_by: instrument` on the `SnapshotRecord` (the caller's
//!   measurement budget, never the subject's).
//! - **Writer sessions only** — every op mints ledger rows; an attach
//!   (read-only) session is `session_is_read_only`.
//! - **Fail-closed** — `set_phase` refuses unless the handle declares
//!   `per_phase_network_policy` *and* the sealed policy's
//!   `ext["phase_schedule"]` names the phase (CF-318; ADR-0142).

use hh_embed_schema::errors::EmbedError;
use hh_env::handle::{DeriveMode, OnParentEnd};
use hh_env::snapshot::TakenBy;
use hh_ledger::store::Lease;
use hh_wire::json::Json;

use crate::service::EmbedService;

impl EmbedService {
    /// The shared env-op subject: a live *writer* session's
    /// `(run_id, lease, env_handle_id)` — an attach session or a
    /// session without an environment is the typed refusal, never a
    /// guess.
    fn env_subject(
        &mut self,
        params: &Json,
        op: &str,
    ) -> Result<(String, Lease, String), EmbedError> {
        // Lenient extraction — the op's own members (`phase`, `mode`,
        // `scope`, …) are present alongside `session_id`; a StrictObj
        // here would refuse them as unknown.
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
            s.env_handle_id.clone().ok_or(EmbedError::Refused {
                reason: "no_environment".to_string(),
            })?,
        ))
    }

    /// `env.snapshot{session_id}` — an `fs_tree` snapshot of the
    /// session's environment, `taken_by = instrument` (ADR-0177 D7).
    /// Returns `{snapshot_ref, kind, at_seq, size_bytes, taken_by}` —
    /// the record's own fields, no handle id.
    pub(crate) fn env_snapshot(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, lease, env_handle_id) = self.env_subject(params, "env.snapshot")?;
        let driver = self.env_drivers.get_mut(&run_id).ok_or_else(|| {
            EmbedError::EnvironmentUnavailable {
                reason: "env_driver_absent".to_string(),
            }
        })?;
        let (rec, _tree) = driver
            .fs_tree_snapshot_as(&mut self.store, &lease, &env_handle_id, TakenBy::Instrument)
            .map_err(crate::open::env_err)?;
        Ok(Json::obj([
            ("snapshot_ref", Json::str(rec.snapshot_ref)),
            ("kind", Json::str("fs_tree")),
            ("at_seq", Json::Int(rec.at_seq as i64)),
            ("size_bytes", Json::Int(rec.size_bytes as i64)),
            ("taken_by", Json::str("instrument")),
        ]))
    }

    /// `env.derive{session_id, mode, scope?, on_parent_end?}` — derive a
    /// child environment from the session's handle (ADR-0137 §5). The
    /// child's handle id stays kernel-side (R-NOSIDE); the result is the
    /// honest record — `{derived, mode, state, class}`.
    pub(crate) fn env_derive(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, lease, env_handle_id) = self.env_subject(params, "env.derive")?;
        let mode_str = params
            .get("mode")
            .and_then(Json::as_str)
            .unwrap_or("fork_snapshot")
            .to_string();
        let mode = match mode_str.as_str() {
            "fresh_from_image" => DeriveMode::FreshFromImage,
            "fork_snapshot" => DeriveMode::ForkSnapshot,
            "scoped_subtree" => DeriveMode::ScopedSubtree,
            "share" => DeriveMode::Share,
            other => {
                return Err(EmbedError::SchemaViolation {
                    path: "env.derive/mode".to_string(),
                    code: format!("unknown_derive_mode:{other}"),
                })
            }
        };
        let scope = params.get("scope").and_then(Json::as_str);
        let on_parent_end = match params
            .get("on_parent_end")
            .and_then(Json::as_str)
            .unwrap_or("teardown")
        {
            "teardown" => OnParentEnd::Teardown,
            "detach_to_child" => OnParentEnd::DetachToChild,
            other => {
                return Err(EmbedError::SchemaViolation {
                    path: "env.derive/on_parent_end".to_string(),
                    code: format!("unknown_on_parent_end:{other}"),
                })
            }
        };
        let driver = self.env_drivers.get_mut(&run_id).ok_or_else(|| {
            EmbedError::EnvironmentUnavailable {
                reason: "env_driver_absent".to_string(),
            }
        })?;
        let child = driver
            .derive(
                &mut self.store,
                &lease,
                &env_handle_id,
                mode,
                scope,
                on_parent_end,
            )
            .map_err(crate::open::env_err)?;
        Ok(Json::obj([
            ("derived", Json::Bool(true)),
            ("mode", Json::str(mode_str)),
            ("state", Json::str(child.state.as_str())),
            ("class", Json::str(child.class.as_str())),
        ]))
    }

    /// `env.set_phase{session_id, phase}` — apply the sealed phase
    /// schedule (ADR-0142). Fail-closed: `Refused{phase_schedule_
    /// undeclared}` when the handle does not declare
    /// `per_phase_network_policy` or the policy names no schedule entry
    /// for the phase — the surface never swaps a policy the record did
    /// not seal.
    pub(crate) fn env_set_phase(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, lease, env_handle_id) = self.env_subject(params, "env.set_phase")?;
        let phase = params
            .get("phase")
            .and_then(Json::as_str)
            .ok_or_else(|| EmbedError::SchemaViolation {
                path: "env.set_phase/phase".to_string(),
                code: "missing".to_string(),
            })?
            .to_string();
        let driver = self.env_drivers.get_mut(&run_id).ok_or_else(|| {
            EmbedError::EnvironmentUnavailable {
                reason: "env_driver_absent".to_string(),
            }
        })?;
        driver
            .set_phase(&mut self.store, &lease, &env_handle_id, &phase)
            .map_err(|e| match e {
                hh_env::errors::EnvError::Unsupported { capability, .. }
                    if capability == "per_phase_network_policy"
                        || capability == "phase_schedule" =>
                {
                    EmbedError::Refused {
                        reason: "phase_schedule_undeclared".to_string(),
                    }
                }
                other => crate::open::env_err(other),
            })?;
        Ok(Json::obj([
            ("phase", Json::str(phase)),
            ("applied", Json::Bool(true)),
        ]))
    }
}
