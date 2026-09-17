//! `EnvDriver` — the kernel-side environment manager (§5a.5 §4 surface). It
//! owns the `EnvHandle`s, drives the `declared → provisioning → ready →
//! torn_down` machine, runs fail-closed `attach` through `hh-containment`,
//! applies `env_apply` (placeholders only), and is the only reachability path
//! for the fs/`upload`/`download`/`path_baseline` effect surface (R-NOSIDE —
//! every op is kernel-mediated through a `ready`, verified handle).
//!
//! The driver holds no `Store` borrow — every method takes `store` + `lease`
//! so the `Dispatcher` (which also appends) can share one writer. Kernel death
//! is *not* environment death: `mark_unreachable` + `heal` recover a handle
//! whose session outlived a kernel restart (AC-R-2.2.5-2) — `active_ms`
//! excludes the gap.

use std::collections::BTreeMap;

use hh_containment::attach::{
    attach, AttachInput, AttachMode, AttachOutcome, PolicySlot, SealWarning,
};
use hh_containment::backend::ContainmentBackend;
use hh_containment::report::ContainmentReport;
use hh_hir::records::Grant;
use hh_ledger::manifest::EventRef;
use hh_ledger::store::{Lease, Store};

use crate::errors::EnvError;
use crate::events::{self, EventMinter};
use crate::handle::{
    EnvCapabilityDeclaration, EnvHandle, EnvSession, HandleState, Health, OnLoss, Roots,
    SnapshotCadence,
};
use crate::record::{EnvironmentRecord, ResolvedImage};
use crate::snapshot::{PathBaseline, SnapshotKind, SnapshotRecord, TakenBy};

/// `EnvDriver` — the environment manager (owns the handle table; borrows the
/// store per call).
pub struct EnvDriver {
    run_id: String,
    /// The live handles (`env_handle_id → handle`).
    handles: BTreeMap<String, EnvHandle>,
}

impl EnvDriver {
    /// A driver for `run_id`.
    pub fn new(run_id: &str) -> Self {
        EnvDriver {
            run_id: run_id.to_string(),
            handles: BTreeMap::new(),
        }
    }

    /// A handle by id.
    pub fn handle(&self, env_handle_id: &str) -> Option<&EnvHandle> {
        self.handles.get(env_handle_id)
    }

    /// A mutable handle by id.
    pub fn handle_mut(&mut self, env_handle_id: &str) -> Option<&mut EnvHandle> {
        self.handles.get_mut(env_handle_id)
    }

    /// Install a handle (tests build the table directly).
    pub fn install(&mut self, h: EnvHandle) {
        self.handles.insert(h.env_handle_id.clone(), h);
    }

    /// `provision(record, roots, containment, on_loss) → EnvHandle` —
    /// `resolve` the image (UnresolvedRef on an unpinned tag), `identify` the
    /// record (the `environment_ref`), mint the handle, append `declared` +
    /// `provisioning` + `provisioned`. The handle lands in `provisioning`;
    /// `attach` completes the path to `ready`.
    pub fn provision(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        record: &EnvironmentRecord,
        roots: Roots,
        containment: PolicySlot,
        on_loss: OnLoss,
    ) -> Result<EnvHandle, EnvError> {
        if !record.class.stage1_supported() {
            return Err(EnvError::Unsupported {
                capability: "provision",
                detail: format!(
                    "class {} is not Stage-1 provisionable",
                    record.class.as_str()
                ),
            });
        }
        let image = record.resolve()?;
        let identity = record.identify()?;
        let environment_ref = (
            identity
                .semantic_id
                .clone()
                .unwrap_or_else(|| identity.version_id.clone()),
            identity.version_id.clone(),
        );
        let now = store.now_ms();
        let capabilities = match record.class {
            crate::record::EnvironmentClass::LocalHost => {
                EnvCapabilityDeclaration::stage1_local_host()
            }
            _ => EnvCapabilityDeclaration::stage1_local_sandboxed(),
        };
        let handle = EnvHandle {
            env_handle_id: store.alloc_id("env"),
            run_id: self.run_id.clone(),
            environment_ref,
            class: record.class,
            state: HandleState::Declared,
            health: Health::Unknown,
            session: None,
            capabilities,
            containment,
            report: None,
            credential_bindings: vec![],
            roots,
            limits: record.limits.clone(),
            budget_node_refs: vec![],
            meters: crate::handle::EnvMeters::new(now),
            snapshots: vec![],
            parent: None,
            on_loss,
            snapshot_cadence: SnapshotCadence::Never,
            heal_count: 0,
            image,
            applied_event_ref: None,
            created_ms: now,
        };
        let declared = EventMinter::new(store, &self.run_id).mint(
            "action.environment.declared",
            events::declared_payload(&handle),
        )?;
        store.append(&self.run_id, lease, vec![declared])?;
        let mut h = handle;
        h.transition(HandleState::Provisioning, now)?;
        let provisioning = EventMinter::new(store, &self.run_id).mint(
            "action.environment.provisioning",
            events::provisioning_payload(&h),
        )?;
        let provisioned = EventMinter::new(store, &self.run_id).mint(
            "action.environment.provisioned",
            events::provisioned_payload(&h),
        )?;
        store.append(&self.run_id, lease, vec![provisioning, provisioned])?;
        self.handles.insert(h.env_handle_id.clone(), h.clone());
        Ok(h)
    }

    /// `attach(env_handle_id, backend, mode, lab_run, grants) → (report,
    /// warnings)` — the fail-closed DF-S1.12-3 seam: run
    /// `hh_containment::attach`, store the `ContainmentReport`, append the
    /// `security.containment.applied`/`unverified` row, then `attached`
    /// (naming the applied event) + `ready`, install the session.
    #[allow(clippy::too_many_arguments)]
    pub fn attach(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
        backend: Option<&dyn ContainmentBackend>,
        mode: AttachMode,
        lab_run: bool,
        grants: &[(String, Grant)],
    ) -> Result<(ContainmentReport, Vec<SealWarning>), EnvError> {
        let now = store.now_ms();
        let h = self
            .handles
            .get(env_handle_id)
            .ok_or_else(|| EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?
            .clone();
        if h.state != HandleState::Provisioning && h.state != HandleState::Detached {
            return Err(EnvError::InvalidState {
                op: "attach",
                state: h.state.as_str(),
            });
        }
        let outcome = attach(&AttachInput {
            env_handle: env_handle_id,
            policy: h.containment.clone(),
            backend,
            mode,
            lab_run,
            resume_report: h.report.as_ref(),
            grants,
        })?;
        let (report, applied_event_id, warnings) = match outcome {
            AttachOutcome::Applied {
                report,
                event,
                warnings,
            } => {
                let ev = EventMinter::new(store, &self.run_id)
                    .mint("security.containment.applied", event)?;
                let applied_id = ev.event_id.clone();
                store.append(&self.run_id, lease, vec![ev])?;
                (report, applied_id, warnings)
            }
            AttachOutcome::Degraded {
                report,
                unverified_event,
                warnings,
            } => {
                let ev = EventMinter::new(store, &self.run_id)
                    .mint("security.containment.unverified", unverified_event)?;
                store.append(&self.run_id, lease, vec![ev])?;
                let r = report.ok_or_else(|| EnvError::ContainmentUnverified {
                    field_group: "attach".to_string(),
                    reason: "degraded_no_report".to_string(),
                })?;
                let h2 = self.handles.get_mut(env_handle_id).unwrap();
                h2.report = Some(r);
                return Err(EnvError::ContainmentUnverified {
                    field_group: "degraded".to_string(),
                    reason: format!("warn_and_degrade (warnings {})", warnings.len()),
                });
            }
        };
        let h = self.handles.get_mut(env_handle_id).unwrap();
        h.report = Some(report.clone());
        h.applied_event_ref = Some(applied_event_id.clone());
        h.session = Some(EnvSession {
            session_id: store.alloc_id("sess"),
            helper_identity: "local".to_string(),
            attached_at_ms: now,
            lease_generation: lease.generation,
        });
        h.health = Health::Healthy;
        h.transition(HandleState::Ready, now)?;
        let mut attached = EventMinter::new(store, &self.run_id).mint(
            "action.environment.attached",
            events::attached_payload(h, &applied_event_id),
        )?;
        attached.causes = vec![EventRef {
            run_id: self.run_id.clone(),
            event_id: applied_event_id.clone(),
        }];
        let ready = EventMinter::new(store, &self.run_id)
            .mint("action.environment.ready", events::ready_payload(h))?;
        store.append(&self.run_id, lease, vec![attached, ready])?;
        Ok((report, warnings))
    }

    /// `detach(env_handle_id)` — `ready → detached`, release the session.
    pub fn detach(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
    ) -> Result<(), EnvError> {
        let now = store.now_ms();
        let h = self
            .handles
            .get_mut(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?;
        h.verify_environment()?;
        h.transition(HandleState::Detached, now)?;
        h.session = None;
        let ev = EventMinter::new(store, &self.run_id).mint(
            "action.environment.detached",
            events::detached_payload(h, "released"),
        )?;
        store.append(&self.run_id, lease, vec![ev])?;
        Ok(())
    }

    /// `teardown(env_handle_id)` — any live state → `torn_down` (terminal).
    pub fn teardown(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
    ) -> Result<(), EnvError> {
        let now = store.now_ms();
        let h = self
            .handles
            .get_mut(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?;
        if h.state.is_terminal() {
            return Err(EnvError::InvalidState {
                op: "teardown",
                state: h.state.as_str(),
            });
        }
        h.transition(HandleState::TornDown, now)?;
        h.session = None;
        let ev = EventMinter::new(store, &self.run_id).mint(
            "action.environment.torn_down",
            events::torn_down_payload(h, "teardown"),
        )?;
        store.append(&self.run_id, lease, vec![ev])?;
        Ok(())
    }

    /// `mark_unreachable(env_handle_id)` — contact lost. `ready →
    /// unreachable`; `active_ms` stops at `last_contact` (the kernel-dead gap
    /// is excluded — AC-R-2.2.5-2).
    pub fn mark_unreachable(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
    ) -> Result<(), EnvError> {
        let now = store.now_ms();
        let h = self
            .handles
            .get_mut(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?;
        if h.state != HandleState::Ready {
            return Err(EnvError::InvalidState {
                op: "mark_unreachable",
                state: h.state.as_str(),
            });
        }
        h.meters.mark_unreachable(now);
        h.transition(HandleState::Unreachable, now)?;
        let ev = EventMinter::new(store, &self.run_id).mint(
            "action.environment.unreachable",
            events::unreachable_payload(h),
        )?;
        store.append(&self.run_id, lease, vec![ev])?;
        Ok(())
    }

    /// `heal(env_handle_id, session_still_live)` — decide `reattached → ready`
    /// (within `reattach_window_ms` and the session is live) or `failed`
    /// (`on_loss` / window lapsed). Kernel death ≠ environment death: a
    /// session that outlived the kernel reattaches; only a truly dead
    /// environment fails.
    pub fn heal(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
        session_still_live: bool,
    ) -> Result<(), EnvError> {
        let now = store.now_ms();
        let h = self
            .handles
            .get_mut(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?;
        if h.state != HandleState::Unreachable {
            return Err(EnvError::InvalidState {
                op: "heal",
                state: h.state.as_str(),
            });
        }
        let gap = now.saturating_sub(h.meters.last_contact());
        let window = h.capabilities.reattach_window_ms.unwrap_or(0);
        if !session_still_live || gap > window {
            h.transition(HandleState::Failed, now)?;
            h.session = None;
            let ev = EventMinter::new(store, &self.run_id).mint(
                "action.environment.failed",
                events::failed_payload(h, "heal_unrecoverable"),
            )?;
            store.append(&self.run_id, lease, vec![ev])?;
            return Err(EnvError::WindowLapsed {
                env_handle_id: env_handle_id.to_string(),
                window_ms: window,
            });
        }
        h.heal_count += 1;
        h.transition(HandleState::Reattached, now)?;
        h.meters.reattach(now);
        h.transition(HandleState::Ready, now)?;
        let ev = EventMinter::new(store, &self.run_id).mint(
            "action.environment.reattached",
            events::reattached_payload(h),
        )?;
        store.append(&self.run_id, lease, vec![ev])?;
        Ok(())
    }

    /// `verify(env_handle_id)` — the explicit verification point (appends the
    /// `verified` audit row; the per-use gate is
    /// `EnvHandle::verify_environment`, pure).
    pub fn verify(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
    ) -> Result<(), EnvError> {
        let h = self
            .handles
            .get(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?;
        h.verify_environment()?;
        let report_fresh = h.report.is_some();
        let ev = EventMinter::new(store, &self.run_id).mint(
            "action.environment.verified",
            events::verified_payload(h, report_fresh),
        )?;
        store.append(&self.run_id, lease, vec![ev])?;
        Ok(())
    }

    /// `meters_sampled(env_handle_id)` — record the three kernel clocks.
    pub fn sample_meters(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
    ) -> Result<(), EnvError> {
        let now = store.now_ms();
        let h = self
            .handles
            .get(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?;
        let ev = EventMinter::new(store, &self.run_id).mint(
            "action.environment.meters_sampled",
            events::meters_sampled_payload(h, now),
        )?;
        store.append(&self.run_id, lease, vec![ev])?;
        Ok(())
    }

    // ── the fs effect surface (R-NOSIDE — kernel-mediated, verified) ────────

    /// `fs_read(env_handle_id, path)` — read a file inside a readable root.
    pub fn fs_read(&self, env_handle_id: &str, path: &str) -> Result<Vec<u8>, EnvError> {
        let h = self
            .handles
            .get(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?;
        h.verify_environment()?;
        let canon = canonicalize(path);
        if !h.roots.is_readable(&canon) {
            return Err(EnvError::OutsideRoots { path: canon });
        }
        std::fs::read(&canon).map_err(|e| EnvError::Blob(format!("fs_read {canon}: {e}")))
    }

    /// `fs_write(env_handle_id, path, bytes)` — write inside a writable root.
    pub fn fs_write(&self, env_handle_id: &str, path: &str, bytes: &[u8]) -> Result<(), EnvError> {
        let h = self
            .handles
            .get(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?;
        h.verify_environment()?;
        let canon = canonicalize(path);
        if !h.roots.is_writable(&canon) {
            return Err(EnvError::OutsideRoots { path: canon });
        }
        std::fs::write(&canon, bytes).map_err(|e| EnvError::Blob(format!("fs_write {canon}: {e}")))
    }

    /// `fs_list(env_handle_id, path)` — list a directory inside a readable root.
    pub fn fs_list(&self, env_handle_id: &str, path: &str) -> Result<Vec<String>, EnvError> {
        let h = self
            .handles
            .get(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?;
        h.verify_environment()?;
        let canon = canonicalize(path);
        if !h.roots.is_readable(&canon) {
            return Err(EnvError::OutsideRoots { path: canon });
        }
        let mut out = Vec::new();
        let rd = std::fs::read_dir(&canon).map_err(|e| EnvError::Blob(format!("fs_list: {e}")))?;
        for e in rd.flatten() {
            out.push(e.path().to_string_lossy().to_string());
        }
        out.sort();
        Ok(out)
    }

    /// `upload(env_handle_id, path) → ContentAddress` — store a file's bytes
    /// into the blob pool (the path must be readable).
    pub fn upload(
        &mut self,
        store: &mut Store,
        env_handle_id: &str,
        path: &str,
    ) -> Result<hh_identity::idp::ContentAddress, EnvError> {
        let bytes = self.fs_read(env_handle_id, path)?;
        store
            .put_blob(&bytes, "application/octet-stream")
            .map_err(EnvError::Ledger)
    }

    /// `download(env_handle_id, address, path)` — materialise a blob into a
    /// writable root.
    pub fn download(
        &mut self,
        store: &mut Store,
        env_handle_id: &str,
        address: &hh_identity::idp::ContentAddress,
        path: &str,
    ) -> Result<(), EnvError> {
        let bytes = store.get_blob(address).map_err(EnvError::Ledger)?;
        self.fs_write(env_handle_id, path, &bytes)
    }

    /// `path_baseline(env_handle_id)` — the `path_baseline` snapshot over the
    /// readable roots (the capture's fs-diff baseline). Appends
    /// `action.environment.snapshot`.
    pub fn path_baseline(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
    ) -> Result<(SnapshotRecord, PathBaseline), EnvError> {
        let (roots, image_base) = {
            let h = self
                .handles
                .get(env_handle_id)
                .ok_or(EnvError::Unavailable {
                    env_handle_id: env_handle_id.to_string(),
                    state: "missing",
                })?;
            h.verify_environment()?;
            h.capabilities
                .snapshot_supported(SnapshotKind::PathBaseline)?;
            let base = match &h.image {
                ResolvedImage::Address(ca) => Some(ca.id()),
                _ => None,
            };
            (h.roots.workspace_roots.clone(), base)
        };
        let mut paths = Vec::new();
        for r in &roots {
            collect_files(r, &mut paths);
        }
        let baseline = PathBaseline::take(&|p| std::fs::read(p).ok(), &paths);
        let mut rec = SnapshotRecord {
            snapshot_ref: String::new(),
            env_handle_id: env_handle_id.to_string(),
            at_seq: 0,
            kind: SnapshotKind::PathBaseline,
            base: image_base,
            content: baseline.to_json(),
            roots_covered: roots,
            quiesced: false,
            taken_by: TakenBy::Subject,
            size_bytes: baseline.entries.len() as u64,
            expires_at_ms: None,
        };
        rec.snapshot_ref = rec.compute_ref();
        let h = self.handles.get_mut(env_handle_id).unwrap();
        h.snapshots.push(rec.snapshot_ref.clone());
        let ev = EventMinter::new(store, &self.run_id).mint(
            "action.environment.snapshot",
            events::snapshot_payload(h, &rec.snapshot_ref, "path_baseline"),
        )?;
        store.append(&self.run_id, lease, vec![ev])?;
        Ok((rec, baseline))
    }
}

/// Canonicalize a path (lexical `.`/`..` resolution + symlink resolution when
/// the path exists — symlink escapes resolve *before* the root check, F1).
fn canonicalize(path: &str) -> String {
    match std::fs::canonicalize(path) {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => {
            let mut out = Vec::new();
            for seg in path.split('/') {
                match seg {
                    "" | "." => {}
                    ".." => {
                        out.pop();
                    }
                    s => out.push(s),
                }
            }
            format!("/{}", out.join("/"))
        }
    }
}

/// Recursively collect regular-file paths under `root`.
fn collect_files(root: &str, out: &mut Vec<String>) {
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_files(&p.to_string_lossy(), out);
            } else {
                out.push(p.to_string_lossy().to_string());
            }
        }
    }
}
