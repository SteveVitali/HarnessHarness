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
use hh_embed_schema::types::EnvironmentInput;
use hh_env::handle::{DeriveMode, OnParentEnd};
use hh_env::snapshot::TakenBy;
use hh_ledger::store::Lease;
use hh_wire::json::Json;

use crate::service::{ledger_err, EmbedService};

impl EmbedService {
    /// The shared env-op subject: a live *writer* session's
    /// `(run_id, lease, env_handle_id)` — an attach session or a
    /// session without an environment is the typed refusal, never a
    /// guess.
    pub(crate) fn env_subject(
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

    /// `env.snapshot{session_id, kind?}` — a snapshot of the session's
    /// environment, `taken_by = instrument` (ADR-0177 D7). `kind`
    /// defaults `fs_tree` (the workspace-tree rung); `memory` (S5.8;
    /// R-2.2.5²) routes to the provider adapter's checkpoint — declared
    /// `supported` classes only, `quiesced` (`suspended`) handles only;
    /// every other spelling is a `SchemaViolation`, never a guess.
    /// Returns `{snapshot_ref, kind, at_seq, size_bytes, taken_by}` —
    /// the record's own fields, no handle id.
    pub(crate) fn env_snapshot(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, lease, env_handle_id) = self.env_subject(params, "env.snapshot")?;
        let kind = params
            .get("kind")
            .and_then(Json::as_str)
            .unwrap_or("fs_tree")
            .to_string();
        let driver = self.env_drivers.get_mut(&run_id).ok_or_else(|| {
            EmbedError::EnvironmentUnavailable {
                reason: "env_driver_absent".to_string(),
            }
        })?;
        let rec = match kind.as_str() {
            "fs_tree" => {
                driver
                    .fs_tree_snapshot_as(
                        &mut self.store,
                        &lease,
                        &env_handle_id,
                        TakenBy::Instrument,
                    )
                    .map_err(crate::open::env_err)?
                    .0
            }
            "memory" => driver
                .memory_snapshot(&mut self.store, &lease, &env_handle_id, TakenBy::Instrument)
                .map_err(crate::open::env_err)?,
            other => {
                return Err(EmbedError::SchemaViolation {
                    path: "env.snapshot/kind".to_string(),
                    code: format!("unknown_snapshot_kind:{other}"),
                })
            }
        };
        Ok(Json::obj([
            ("snapshot_ref", Json::str(rec.snapshot_ref)),
            ("kind", Json::str(kind)),
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
        // R2.9a (DF-S2.4-1c; LT-09) — a `fork_snapshot` child must not
        // carry the parent's placeholder spellings: capture the parent
        // handle before `derive` so the rebind can name it.
        let parent_handle = (mode == DeriveMode::ForkSnapshot)
            .then(|| driver.handle(&env_handle_id).cloned())
            .flatten();
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
        if let Some(parent) = parent_handle {
            // Fresh placeholders + preserved expiry ceilings; the
            // `security.credential.bound` rows are durable before the
            // derive answers.
            driver
                .rebind_credentials_for_fork(
                    &mut self.store,
                    &lease,
                    &mut self.credential_broker,
                    &parent,
                    &child.env_handle_id,
                )
                .map_err(crate::open::env_err)?;
        }
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

    // ── R2.4 (DF-S2.10-1; §5a.1–5a.2 S1–S5; ADR-0272 residual closed) —─
    // the environment lifecycle family. Every op routes through the
    // session's `EnvDriver` and its declared capabilities — a class
    // without the declaration answers the typed refusal (`Unsupported`/
    // `UnknownCapability`/`InvalidState` → `EnvironmentUnavailable`
    // naming the reason), never a fabricated success. `EnvHandleId`s
    // stay kernel-side (R-NOSIDE) — results carry class/state/refs.

    /// `env.open{session_id, connection_info?}` — provision + attach a
    /// *new* environment on the run and select it as the session's env
    /// (S1: `open` takes `connection_info`; absent → `{"class":
    /// "local_host"}` over the run workspace — the same default `open`
    /// uses). The `action.environment.selected{turn_id, env_handle}`
    /// row records the switch; the previous handle stays live (never
    /// silently torn down).
    pub(crate) fn env_open(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let session_id = params
            .get("session_id")
            .and_then(Json::as_str)
            .ok_or_else(|| EmbedError::SchemaViolation {
                path: "env.open/session_id".into(),
                code: "missing".to_string(),
            })?
            .to_string();
        let s = self.writer_session(&session_id)?;
        let run_id = s.run_id.clone();
        let lease = s.lease.clone().ok_or(EmbedError::Refused {
            reason: "session_is_read_only".to_string(),
        })?;
        let info = params
            .get("connection_info")
            .cloned()
            .unwrap_or_else(|| Json::obj([("class", Json::str("local_host"))]));
        let environment = EnvironmentInput::ConnectionInfo(info);
        let (hid, env_json) = self.provision_environment(&run_id, &lease, &environment)?;
        let hid = hid.ok_or_else(|| EmbedError::EnvironmentUnavailable {
            reason: "env_open: no handle provisioned".to_string(),
        })?;
        let class = {
            let driver = self.env_drivers.get(&run_id).ok_or_else(|| {
                EmbedError::EnvironmentUnavailable {
                    reason: "env_driver_absent".to_string(),
                }
            })?;
            driver
                .handle(&hid)
                .map(|h| h.class.as_str().to_string())
                .unwrap_or_default()
        };
        let turn_id = self
            .session(&session_id)
            .map(|s| s.active_turn.clone())
            .unwrap_or_else(|_| "turn-1".to_string());
        self.mint(
            &run_id,
            &lease,
            "action.environment.selected",
            Json::obj([
                ("env_handle", Json::str(&hid)),
                ("turn_id", Json::str(turn_id)),
            ]),
        )?;
        let s = self.session_mut(&session_id)?;
        s.env_handle_id = Some(hid);
        s.env_json = env_json;
        Ok(Json::obj([
            ("opened", Json::Bool(true)),
            ("class", Json::str(class)),
            ("state", Json::str("ready")),
        ]))
    }

    /// `env.attach{session_id, substrate_attestation?}` — reattach the
    /// session's env handle (the `detached`/`provisioning` states admit;
    /// anything else is the driver's typed `InvalidState`). The backend
    /// re-derives from the handle's own containment slot
    /// (`for_policy`) — the attesting isolation classes need the
    /// binding's `substrate_attestation` again (`attestation_missing`
    /// closes the path honestly). Provider-class handles refuse
    /// `UnknownCapability{env.attach}` — the kernel-side containment
    /// attach is a local-class mechanism.
    pub(crate) fn env_attach(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, lease, env_handle_id) = self.env_subject(params, "env.attach")?;
        let attestation = match params.get("substrate_attestation") {
            None => None,
            Some(j) => Some(hh_containment::backend::Attestation {
                method: j
                    .get("method")
                    .and_then(Json::as_str)
                    .ok_or_else(|| EmbedError::SchemaViolation {
                        path: "env.attach/substrate_attestation.method".into(),
                        code: "missing".to_string(),
                    })?
                    .to_string(),
                attestation_ref: j
                    .get("attestation_ref")
                    .and_then(Json::as_str)
                    .ok_or_else(|| EmbedError::SchemaViolation {
                        path: "env.attach/substrate_attestation.attestation_ref".into(),
                        code: "missing".to_string(),
                    })?
                    .to_string(),
            }),
        };
        let driver = self.env_drivers.get_mut(&run_id).ok_or_else(|| {
            EmbedError::EnvironmentUnavailable {
                reason: "env_driver_absent".to_string(),
            }
        })?;
        {
            let h = driver.handle(&env_handle_id).ok_or_else(|| {
                EmbedError::EnvironmentUnavailable {
                    reason: "env_handle_absent".to_string(),
                }
            })?;
            if h.provider.is_some() || h.hosted.is_some() {
                return Err(EmbedError::UnknownCapability {
                    capability: format!("env.attach:{}", h.class.as_str()),
                });
            }
        }
        let backend = hh_containment::backend::for_policy(
            driver.handle(&env_handle_id).unwrap().containment.policy(),
            attestation,
        )
        .map_err(|e| EmbedError::EnvironmentUnavailable {
            reason: format!("attach_backend:{e:?}"),
        })?;
        driver
            .attach(
                &mut self.store,
                &lease,
                &env_handle_id,
                Some(&*backend),
                hh_containment::attach::AttachMode::FailClosed,
                false,
                &[],
            )
            .map_err(crate::open::env_err)?;
        let state = driver
            .handle(&env_handle_id)
            .map(|h| h.state.as_str().to_string())
            .unwrap_or_default();
        Ok(Json::obj([
            ("attached", Json::Bool(true)),
            ("state", Json::str(state)),
        ]))
    }

    /// `env.close{session_id, mode?}` — `teardown` (default) is the
    /// terminal close (`action.environment.torn_down`; the session
    /// unbinds the handle); `mode:"detach"` is the survivable form
    /// (`action.environment.detached` — a later `env.attach` resumes it;
    /// `remote_persistent`/`provider_hosted` keep their remote side, the
    /// `detach` machinery already records that). Any other spelling is a
    /// `SchemaViolation`.
    pub(crate) fn env_close(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, lease, env_handle_id) = self.env_subject(params, "env.close")?;
        let mode = params
            .get("mode")
            .and_then(Json::as_str)
            .unwrap_or("teardown")
            .to_string();
        let driver = self.env_drivers.get_mut(&run_id).ok_or_else(|| {
            EmbedError::EnvironmentUnavailable {
                reason: "env_driver_absent".to_string(),
            }
        })?;
        let state = match mode.as_str() {
            "teardown" => {
                driver
                    .teardown(&mut self.store, &lease, &env_handle_id)
                    .map_err(crate::open::env_err)?;
                "torn_down"
            }
            "detach" => {
                driver
                    .detach(&mut self.store, &lease, &env_handle_id)
                    .map_err(crate::open::env_err)?;
                "detached"
            }
            other => {
                return Err(EmbedError::SchemaViolation {
                    path: "env.close/mode".to_string(),
                    code: format!("unknown_close_mode:{other}"),
                })
            }
        };
        if mode == "teardown" {
            // The handle is terminal — the session unbinds it (a later
            // env op answers `no_environment`, never a stale write).
            self.session_mut(
                params
                    .get("session_id")
                    .and_then(Json::as_str)
                    .unwrap_or_default(),
            )?
            .env_handle_id = None;
        }
        Ok(Json::obj([
            ("closed", Json::Bool(true)),
            ("mode", Json::str(mode)),
            ("state", Json::str(state)),
        ]))
    }

    /// `env.diff{session_id, baseline}` — the change-set between the
    /// named `fs_tree` snapshot and the live roots (S2 `diff` —
    /// `fs_read`-class, read-only over the fs surface). The baseline is
    /// named by `snapshot_ref` — a missing/foreign/GC'd ref answers the
    /// typed `SnapshotMissing` refusal.
    pub(crate) fn env_diff(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, _lease, env_handle_id) = self.env_subject(params, "env.diff")?;
        let baseline = params
            .get("baseline")
            .and_then(Json::as_str)
            .ok_or_else(|| EmbedError::SchemaViolation {
                path: "env.diff/baseline".into(),
                code: "missing".to_string(),
            })?
            .to_string();
        let driver =
            self.env_drivers
                .get(&run_id)
                .ok_or_else(|| EmbedError::EnvironmentUnavailable {
                    reason: "env_driver_absent".to_string(),
                })?;
        let entries = driver
            .fs_tree_diff(&self.store, &env_handle_id, &baseline)
            .map_err(crate::open::env_err)?;
        let mut rendered = Vec::new();
        for e in &entries {
            let mut m = vec![
                ("root", Json::str(&e.root)),
                ("relpath", Json::str(&e.relpath)),
                ("change", Json::str(e.change)),
            ];
            if let Some(b) = &e.before {
                m.push(("before", Json::str(b)));
            }
            if let Some(a) = &e.after {
                m.push(("after", Json::str(a)));
            }
            rendered.push(Json::obj(m));
        }
        Ok(Json::obj([
            ("baseline", Json::str(baseline)),
            ("entries", Json::Arr(rendered)),
            ("count", Json::Int(entries.len() as i64)),
        ]))
    }

    /// `env.restore{session_id, snapshot_ref, mode?}` — §5a.2 S2: the
    /// default is the *successor* restore (a fresh workspace materialised
    /// from the snapshot's blob-pool content, attached + verified; the
    /// `restored{to_env_handle_id}`/`replaced` rows land and the session
    /// rebinds to the successor). `mode:"in_place"` is the provider
    /// mechanism — gated on `restore_in_place = supported`, the typed
    /// `UnknownCapability`/`Unsupported` surfaces verbatim elsewhere
    /// (local_host never fabricates an in-place revert).
    pub(crate) fn env_restore(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, lease, env_handle_id) = self.env_subject(params, "env.restore")?;
        let snapshot_ref = params
            .get("snapshot_ref")
            .and_then(Json::as_str)
            .ok_or_else(|| EmbedError::SchemaViolation {
                path: "env.restore/snapshot_ref".into(),
                code: "missing".to_string(),
            })?
            .to_string();
        // The named snapshot's kind chooses the default mode (`memory`
        // restores in place — the provider owns the image; `fs_tree`
        // restores by successor).
        let kind = self
            .store
            .env_snapshots(&run_id)
            .map_err(ledger_err)?
            .iter()
            .find(|(_, sr, _, _)| *sr == snapshot_ref)
            .map(|(_, _, kind, _)| kind.clone())
            .ok_or_else(|| EmbedError::EnvironmentUnavailable {
                reason: format!("SnapshotMissing: {snapshot_ref}"),
            })?;
        let mode = match params.get("mode").and_then(Json::as_str) {
            None => {
                if kind == "memory" {
                    "in_place".to_string()
                } else {
                    "successor".to_string()
                }
            }
            Some(m @ ("successor" | "in_place")) => m.to_string(),
            Some(other) => {
                return Err(EmbedError::SchemaViolation {
                    path: "env.restore/mode".to_string(),
                    code: format!("unknown_restore_mode:{other}"),
                })
            }
        };
        let driver = self.env_drivers.get_mut(&run_id).ok_or_else(|| {
            EmbedError::EnvironmentUnavailable {
                reason: "env_driver_absent".to_string(),
            }
        })?;
        match mode.as_str() {
            "in_place" => {
                driver
                    .restore_in_place(&mut self.store, &lease, &env_handle_id, &snapshot_ref)
                    .map_err(crate::open::env_err)?;
                Ok(Json::obj([
                    ("restored", Json::Bool(true)),
                    ("mode", Json::str("in_place")),
                    ("snapshot_ref", Json::str(snapshot_ref)),
                ]))
            }
            _ => {
                if kind != "fs_tree" {
                    return Err(EmbedError::Unsupported {
                        by: format!("env.restore.successor:{kind}"),
                    });
                }
                let child = driver
                    .restore_successor(&mut self.store, &lease, &env_handle_id, &snapshot_ref)
                    .map_err(crate::open::env_err)?;
                let session_id = params
                    .get("session_id")
                    .and_then(Json::as_str)
                    .unwrap_or_default();
                self.session_mut(session_id)?.env_handle_id = Some(child.env_handle_id.clone());
                Ok(Json::obj([
                    ("restored", Json::Bool(true)),
                    ("mode", Json::str("successor")),
                    ("snapshot_ref", Json::str(snapshot_ref)),
                    ("verified", Json::Bool(true)),
                ]))
            }
        }
    }

    /// `env.upload{session_id, path, content? | content_address?}` —
    /// §5a.2's `upload(tree | bytes → path)`: the bytes land in a
    /// writable root *and* the blob pool (content-addressed both ways —
    /// the `action.environment.uploaded` audit row carries the address).
    /// Exactly one of `content` (utf-8 bytes) / `content_address` names
    /// the source. Provider-class handles refuse `fs.upload` — the
    /// kernel fs surface never reaches a remote environment.
    pub(crate) fn env_upload(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, lease, env_handle_id) = self.env_subject(params, "env.upload")?;
        let path = params
            .get("path")
            .and_then(Json::as_str)
            .ok_or_else(|| EmbedError::SchemaViolation {
                path: "env.upload/path".into(),
                code: "missing".to_string(),
            })?
            .to_string();
        let content = params
            .get("content")
            .and_then(Json::as_str)
            .map(|s| s.as_bytes().to_vec());
        let content_address = params.get("content_address").and_then(Json::as_str);
        if content.is_none() == content_address.is_none() {
            return Err(EmbedError::SchemaViolation {
                path: "env.upload".into(),
                code: "need_exactly_one_of:content|content_address".to_string(),
            });
        }
        let driver = self.env_drivers.get_mut(&run_id).ok_or_else(|| {
            EmbedError::EnvironmentUnavailable {
                reason: "env_driver_absent".to_string(),
            }
        })?;
        let (ca, size) = driver
            .env_upload(
                &mut self.store,
                &lease,
                &env_handle_id,
                &path,
                content.as_deref(),
                content_address,
            )
            .map_err(crate::open::env_err)?;
        Ok(Json::obj([
            ("uploaded", Json::Bool(true)),
            ("path", Json::str(path)),
            ("content_address", Json::str(ca.id())),
            ("size_bytes", Json::Int(size as i64)),
        ]))
    }

    /// `env.download{session_id, path}` — §5a.2's `download(path →
    /// ContentAddress)`: the readable path's bytes move into the blob
    /// pool; the `action.environment.downloaded` audit row mints and the
    /// caller gets the `ContentAddress` (never raw bytes over the
    /// boundary — content is addressed).
    pub(crate) fn env_download(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let (run_id, lease, env_handle_id) = self.env_subject(params, "env.download")?;
        let path = params
            .get("path")
            .and_then(Json::as_str)
            .ok_or_else(|| EmbedError::SchemaViolation {
                path: "env.download/path".into(),
                code: "missing".to_string(),
            })?
            .to_string();
        let driver = self.env_drivers.get_mut(&run_id).ok_or_else(|| {
            EmbedError::EnvironmentUnavailable {
                reason: "env_driver_absent".to_string(),
            }
        })?;
        let (ca, size) = driver
            .env_download(&mut self.store, &lease, &env_handle_id, &path)
            .map_err(crate::open::env_err)?;
        Ok(Json::obj([
            ("downloaded", Json::Bool(true)),
            ("path", Json::str(path)),
            ("content_address", Json::str(ca.id())),
            ("size_bytes", Json::Int(size as i64)),
        ]))
    }
}
