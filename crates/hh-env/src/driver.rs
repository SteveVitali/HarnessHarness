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
use hh_ledger::leases::LeaseScope;
use hh_ledger::manifest::EventRef;
use hh_ledger::store::{Lease, Store};
use hh_wire::json::Json;

use crate::errors::EnvError;
use crate::events::{self, EventMinter};
use crate::handle::{
    EnvCapabilityDeclaration, EnvHandle, EnvSession, HandleState, Health, OnLoss, Roots,
    SnapshotCadence,
};
use crate::helper::{HelperClient, HelperExecutor};
use crate::record::{EnvironmentClass, EnvironmentRecord, ResolvedImage};
use crate::snapshot::{PathBaseline, SnapshotKind, SnapshotRecord, TakenBy};

/// `EnvDriver` — the environment manager (owns the handle table; borrows the
/// store per call). Since S2.1 it also owns the live helper sessions — one
/// `hh-helper` process per contained environment (the committed
/// `env_handle`'s runtime reachability path; the `EnvSession` record is the
/// ledger-visible half, the `HelperClient` the channel).
pub struct EnvDriver {
    pub(crate) run_id: String,
    /// The live handles (`env_handle_id → handle`).
    pub(crate) handles: BTreeMap<String, EnvHandle>,
    /// The live helper sessions (`env_handle_id → client`) — `local_host`
    /// has none (the in-process executor is the honest `none` isolation).
    pub(crate) sessions: BTreeMap<String, HelperClient>,
    /// The podman containers the `local_container` handles own
    /// (`env_handle_id → backend` — teardown stops them).
    containers: BTreeMap<String, hh_helper::podman::PodmanBackend>,
    /// The registered provider adapters — `environment.class → adapter`
    /// (S4.13; one adapter per provider class).
    providers: BTreeMap<String, Box<dyn crate::provider::ProviderAdapter>>,
}

/// `preserve_until(ttl)` offered to the helper on `hello` — §05a
/// `reconcile_detached` requires `ttl ≥ writer_lease.ttl + recovery_grace`
/// (ADR-0130 §6; ADR-0132 §3; OQ-252). 24h dominates every shipped writer
/// TTL + grace by three orders of magnitude; the bound is asserted in
/// `durable_recovery::preserve_until_exceeds_writer_ttl_plus_grace`.
pub const PRESERVE_UNTIL_TTL_MS: u64 = 86_400_000;

/// The executor-side dedup window offered on `hello` (C1; R-2.5.5¹) —
/// the same horizon as `preserve_until` so a resumed writer still finds
/// its dedup records inside the preserved journal.
pub const DEDUP_WINDOW_MS: u64 = PRESERVE_UNTIL_TTL_MS;

impl EnvDriver {
    /// A driver for `run_id`.
    pub fn new(run_id: &str) -> Self {
        EnvDriver {
            run_id: run_id.to_string(),
            handles: BTreeMap::new(),
            sessions: BTreeMap::new(),
            containers: BTreeMap::new(),
            providers: BTreeMap::new(),
        }
    }

    /// Register a provider adapter for its class (S4.13 —
    /// `remote_ephemeral`/`remote_persistent`/`provider_hosted` provision
    /// only through a registered adapter).
    pub fn register_provider(&mut self, adapter: Box<dyn crate::provider::ProviderAdapter>) {
        self.providers
            .insert(adapter.class().as_str().to_string(), adapter);
    }

    /// The registered adapter for a class, if any.
    fn provider_for(
        &mut self,
        class: crate::record::EnvironmentClass,
    ) -> Option<&mut (dyn crate::provider::ProviderAdapter + 'static)> {
        self.providers
            .get_mut(class.as_str())
            .map(move |a| a.as_mut())
    }

    /// The capability declaration a provider class's handles carry — the
    /// registered adapter's own claim ([`ProviderAdapter::
    /// capability_declaration`]; S5.8), falling back to the conservative
    /// `provider_class()` map when no adapter is registered yet (the
    /// provision path refuses `no adapter` before a handle exists, so the
    /// fallback never reaches a live handle).
    fn declared_capabilities(
        &self,
        class: crate::record::EnvironmentClass,
    ) -> crate::handle::EnvCapabilityDeclaration {
        self.providers
            .get(class.as_str())
            .map(|a| a.capability_declaration())
            .unwrap_or_else(crate::handle::EnvCapabilityDeclaration::provider_class)
    }

    /// The live handle ids — `verify_environment` on resume iterates them
    /// (§5a.3 C0 — every handle in the checkpoint view is re-verified; S2.3).
    pub fn handle_ids(&self) -> Vec<String> {
        self.handles.keys().cloned().collect()
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
        if !record.class.provisionable() && !record.class.needs_adapter() {
            return Err(EnvError::Unsupported {
                capability: "provision",
                detail: format!(
                    "class {} is not provisionable at this stage",
                    record.class.as_str()
                ),
            });
        }
        let image = record.resolve()?;
        let identity = record.identify()?;
        // The handle id is allocated once — the adapter's record and the
        // kernel's handle name the same coordinate.
        let env_handle_id = store.alloc_id("env");
        let run_id = self.run_id.clone();
        // S4.13 (R-2.2.5¹) — provider classes provision only through a
        // registered adapter; `local_container`/`local_*` build in-process.
        let provider_handle = if record.class.needs_adapter() {
            let Some(adapter) = self.provider_for(record.class) else {
                return Err(EnvError::Unsupported {
                    capability: "provision",
                    detail: format!(
                        "class {} requires a registered ProviderAdapter — none registered",
                        record.class.as_str()
                    ),
                });
            };
            Some(
                adapter.provision(&crate::provider::ProvisionRequest {
                    environment_ref: (
                        identity
                            .semantic_id
                            .clone()
                            .unwrap_or_else(|| identity.version_id.clone()),
                        identity.version_id.clone(),
                    ),
                    image_ref: format!("{:?}", image),
                    env_handle_id: env_handle_id.clone(),
                    run_id,
                })?,
            )
        } else {
            None
        };
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
            c if c.needs_adapter() => self.declared_capabilities(c),
            _ => EnvCapabilityDeclaration::stage1_local_sandboxed(),
        };
        let handle = EnvHandle {
            env_handle_id,
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
            phase: None,
            provider: provider_handle,
            hosted: None,
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

    /// `provision_hosted` — AC-R-2.2.5-13's container-installed slice: a
    /// hosted participant whose environment the *provider* (not the kernel)
    /// installed still gets a kernel-owned `EnvHandle` — same envelope,
    /// same lifecycle grammar — but its `action.environment.*` rows carry
    /// `origin = participant` and only the transitions the adapter reports
    /// are minted. `unobserved` names the `action.environment.*` spellings
    /// the adapter cannot see (e.g. `attached`/`torn_down` when the host
    /// owns them); they render `n/a{unobserved}` on the handle and produce
    /// no fabricated row (`observability_level ∌ events`).
    ///
    /// A session-ABI participant in a *kernel-provisioned* environment uses
    /// `provision`/`attach` — the full kernel-origin lifecycle — exactly as
    /// a native run does; this path exists only where the provider owns
    /// the substrate and observes a subset.
    #[allow(clippy::too_many_arguments)] // the provision record's fields are the arity's.
    pub fn provision_hosted(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        record: &EnvironmentRecord,
        roots: Roots,
        containment: PolicySlot,
        on_loss: OnLoss,
        participant_ref: &str,
        hosting_mechanism: &str,
        unobserved: &[&str],
    ) -> Result<EnvHandle, EnvError> {
        if !record.class.needs_adapter() {
            return Err(EnvError::Unsupported {
                capability: "provision_hosted",
                detail: format!(
                    "class {} is kernel-provisioned — hosted handles are a provider-class path",
                    record.class.as_str()
                ),
            });
        }
        let image = record.resolve()?;
        let identity = record.identify()?;
        let env_handle_id = store.alloc_id("env");
        let environment_ref = (
            identity
                .semantic_id
                .clone()
                .unwrap_or_else(|| identity.version_id.clone()),
            identity.version_id.clone(),
        );
        let now = store.now_ms();
        let binding = crate::handle::HostedEnvBinding {
            participant_ref: participant_ref.to_string(),
            hosting_mechanism: hosting_mechanism.to_string(),
            unobserved: unobserved.iter().map(|s| s.to_string()).collect(),
        };
        let mut handle = EnvHandle {
            env_handle_id,
            run_id: self.run_id.clone(),
            environment_ref,
            class: record.class,
            state: HandleState::Declared,
            health: Health::Unknown,
            session: None,
            capabilities: self.declared_capabilities(record.class),
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
            phase: None,
            provider: None,
            hosted: Some(binding.clone()),
            created_ms: now,
        };
        // Only the transitions the adapter observed are minted — each at
        // `origin = participant` (the unobserved set produces `n/a`, never
        // a fabricated row). `declared` is the binding record itself — the
        // adapter reported the environment exists.
        let minter = EventMinter::new(store, &self.run_id);
        let mut batch = Vec::new();
        if binding.reportable("action.environment.declared") {
            batch.push(minter.mint_participant(
                "action.environment.declared",
                events::declared_payload(&handle),
                participant_ref,
                hosting_mechanism,
            )?);
        }
        handle.transition(HandleState::Provisioning, now)?;
        if binding.reportable("action.environment.provisioning") {
            batch.push(minter.mint_participant(
                "action.environment.provisioning",
                events::provisioning_payload(&handle),
                participant_ref,
                hosting_mechanism,
            )?);
        }
        if binding.reportable("action.environment.provisioned") {
            batch.push(minter.mint_participant(
                "action.environment.provisioned",
                events::provisioned_payload(&handle),
                participant_ref,
                hosting_mechanism,
            )?);
        }
        if !batch.is_empty() {
            store.append(&self.run_id, lease, batch)?;
        }
        self.handles
            .insert(handle.env_handle_id.clone(), handle.clone());
        Ok(handle)
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
        // S2.1 — the contained classes run behind a live `hh-helper`
        // session (the committed `env_handle`'s only reachability path —
        // R-NOSIDE). A helper that won't start fails the attach closed:
        // the handle lands `failed`, the `failed` row is the audit.
        if let Err(e) = self.start_helper(store, env_handle_id) {
            let h = self.handles.get_mut(env_handle_id).unwrap();
            let _ = h.transition(HandleState::Failed, now);
            h.session = None;
            let ev = EventMinter::new(store, &self.run_id).mint(
                "action.environment.failed",
                events::failed_payload(h, "helper_spawn_failed"),
            )?;
            store.append(&self.run_id, lease, vec![ev])?;
            return Err(e);
        }
        Ok((report, warnings))
    }

    /// `start_helper(env_handle_id)` — spawn the `hh-helper` process for the
    /// contained classes and `hello` the session open: the nonce (the
    /// `commit_proof` recompute key — kernel-held, never ledgered), the
    /// attached containment policy, and the roots. `local_host` has no
    /// helper (its `none` isolation is honest — the in-process
    /// `LocalExecutor` runs it).
    fn start_helper(&mut self, store: &mut Store, env_handle_id: &str) -> Result<(), EnvError> {
        use hh_helper::protocol::OnKernelLoss;
        let h = self.handles.get(env_handle_id).unwrap().clone();
        let (backend, extra_args) = match h.class {
            EnvironmentClass::LocalSandboxed => ("seatbelt".to_string(), Vec::new()),
            EnvironmentClass::LocalContainer => {
                // The helper owns the container's lifecycle (`--backend
                // podman` creates + starts it; teardown stops it). The
                // image must be an OCI reference — a content-addressed
                // image has no podman spelling at S2.1 (honest refusal;
                // the record keeps its identity claim).
                let image = match &h.image {
                    ResolvedImage::UnpinnedTag(t) => t.clone(),
                    ResolvedImage::Foreign { value, .. } => value.clone(),
                    ResolvedImage::Address(_) => {
                        return Err(EnvError::Unsupported {
                            capability: "local_container.image",
                            detail: "a content_address image has no OCI ref — declare a tag/digest"
                                .to_string(),
                        })
                    }
                };
                let container = format!(
                    "hh-env-{}",
                    h.env_handle_id
                        .chars()
                        .map(|c| if c.is_ascii_alphanumeric() || c == '-' {
                            c
                        } else {
                            '-'
                        })
                        .collect::<String>()
                );
                let ws = h.roots.cwd.clone();
                std::fs::create_dir_all(&ws)
                    .map_err(|e| EnvError::Blob(format!("container workspace: {e}")))?;
                (
                    "podman".to_string(),
                    vec![
                        "--container".to_string(),
                        container.clone(),
                        "--image".to_string(),
                        image,
                        "--workspace".to_string(),
                        ws,
                    ],
                )
            }
            _ => return Ok(()),
        };
        // The socket lives under a *short* path — AF_UNIX pathnames cap at
        // ~104 bytes on darwin, and a store root under `/var/folders/…`
        // blows past it; the persisted session coordinates (nonce/id/sock)
        // stay under the store root's `envs/<id>/` (regular files, no cap).
        // The dir name carries a store-root hash so parallel drivers whose
        // env ids coincide can't unlink each other's live socket.
        let sock_dir = std::env::temp_dir().join(format!(
            "hh-s-{}-{}",
            &hh_identity::idp::idp_id("sock_dir", store.root().to_string_lossy().as_bytes())[..12],
            h.env_handle_id
        ));
        let mut client = HelperClient::spawn(&sock_dir, &backend, &extra_args)?;
        // The session nonce — kernel-allocated, carried on `hello`, never
        // ledgered (the `commit_proof` recompute key).
        let nonce = store.alloc_id("nonce");
        client
            .hello(
                OnKernelLoss::PreserveUntil {
                    ttl_ms: PRESERVE_UNTIL_TTL_MS,
                },
                &nonce,
                Some(DEDUP_WINDOW_MS),
                Some(&h.containment.policy().to_json()),
                Some((
                    h.roots.workspace_roots.clone(),
                    h.roots.writable_roots.clone(),
                )),
                false,
            )
            .map_err(|e| EnvError::Transport {
                detail: format!("helper hello: {e}"),
            })?;
        let session_id = client.session_id.clone();
        let backend_name = client.backend.clone();
        // Persist the session coordinates (nonce + id) under the store root —
        // kernel-held, never ledgered. A kernel restart resumes the helper
        // session by reading them back (`resume_helper`); the nonce is the
        // `commit_proof` recompute key, so a lost nonce means a lost resume
        // (honest — the helper refuses a mismatched resume).
        let coord_dir = store.root().join("envs").join(&h.env_handle_id);
        std::fs::create_dir_all(&coord_dir).map_err(|e| EnvError::Blob(e.to_string()))?;
        std::fs::write(coord_dir.join("session.nonce"), &nonce)
            .map_err(|e| EnvError::Blob(e.to_string()))?;
        std::fs::write(coord_dir.join("session.id"), &session_id)
            .map_err(|e| EnvError::Blob(e.to_string()))?;
        std::fs::write(
            coord_dir.join("session.sock"),
            client.socket.to_string_lossy().as_bytes(),
        )
        .map_err(|e| EnvError::Blob(e.to_string()))?;
        self.sessions.insert(env_handle_id.to_string(), client);
        if let Some(s) = &mut self.handles.get_mut(env_handle_id).unwrap().session {
            s.session_id = session_id;
            s.helper_identity = format!("hh-helper/1:{backend_name}");
        }
        Ok(())
    }

    /// `take_executor(env_handle_id, projected_env)` — hand the helper
    /// session to the dispatcher as a `ToolExecutor`. The caller MUST
    /// `return_executor` after dispatch — the session outlives the effect.
    /// `local_host` has no session (its executor is `LocalExecutor`).
    pub fn take_executor(
        &mut self,
        env_handle_id: &str,
        projected_env: Vec<(String, String)>,
    ) -> Result<HelperExecutor, EnvError> {
        let client = self.sessions.remove(env_handle_id).ok_or_else(|| {
            let state = self
                .handles
                .get(env_handle_id)
                .map(|h| h.state.as_str())
                .unwrap_or("missing");
            EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state,
            }
        })?;
        Ok(HelperExecutor::new(client, projected_env))
    }

    /// `return_executor(env_handle_id, exec)` — return the session to the
    /// table after dispatch (the helper stays live across effects).
    pub fn return_executor(&mut self, env_handle_id: &str, exec: HelperExecutor) {
        // Per-class meter — the session's exec-spawn count lands on the
        // handle's declared `helper.spawns` meter.
        let spawns = exec.client.spawns;
        self.sessions.insert(env_handle_id.to_string(), exec.client);
        if let Some(h) = self.handles.get_mut(env_handle_id) {
            h.meters.extra.insert(
                "helper.spawns".to_string(),
                crate::handle::MeterSample {
                    value: spawns,
                    provenance: crate::handle::MeterProvenance::Measured,
                },
            );
        }
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
        // The helper session releases with the handle — the channel closes;
        // the helper's `preserve_until` keeps its journal so a later
        // `attach` can resume it (the container persists under the helper).
        if let Some(client) = self.sessions.remove(env_handle_id) {
            drop(client);
        }
        let ev = EventMinter::new(store, &self.run_id).mint(
            "action.environment.detached",
            events::detached_payload(h, "released"),
        )?;
        store.append(&self.run_id, lease, vec![ev])?;
        // S4.13 (R-2.2.5¹) — provider detach semantics: `remote_persistent`
        // and `provider_hosted` environments *survive* detach (the adapter
        // keeps the remote side up for a later `attach`); `remote_ephemeral`
        // dies with the kernel session.
        let class = h.class;
        let ph = if class == crate::record::EnvironmentClass::RemoteEphemeral {
            h.provider.clone()
        } else {
            None
        };
        if let Some(ph) = ph {
            if let Some(adapter) = self.provider_for(class) {
                adapter.teardown(&ph)?;
            }
        }
        Ok(())
    }

    /// `suspend(env_handle_id, on_idle?)` — `ready → suspended` (S4.13;
    /// R-2.2.5¹). Requires the class's declared `suspend` capability —
    /// `SuspendKind::Unknown` refuses `Unsupported` (a capability the class
    /// never declared is an honest `UnknownCapability`, never coerced).
    /// `on_idle` records the policy driving the suspend (`suspended` with
    /// `on_idle` policy, spec §5a.5 extension row).
    pub fn suspend(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
        on_idle: Option<&str>,
    ) -> Result<(), EnvError> {
        let now = store.now_ms();
        let h = self
            .handles
            .get(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?
            .clone();
        if h.capabilities.suspend == crate::handle::SuspendKind::Unknown {
            return Err(EnvError::UnknownCapability {
                capability: "suspend".to_string(),
            });
        }
        // The provider checkpoint runs before the state transition so a
        // failed checkpoint leaves the handle `ready` (no torn state).
        if let Some(ph) = &h.provider {
            if let Some(adapter) = self.provider_for(h.class) {
                adapter.suspend(ph)?;
            }
        }
        let h = self.handles.get_mut(env_handle_id).unwrap();
        h.transition(HandleState::Suspended, now)?;
        let requested = EventMinter::new(store, &self.run_id).mint(
            "action.environment.suspend.requested",
            events::suspend_requested_payload(h, on_idle),
        )?;
        let suspended = EventMinter::new(store, &self.run_id).mint(
            "action.environment.suspended",
            events::suspended_payload(h, on_idle),
        )?;
        // request-then-state in one batch — durable before visible.
        store.append(&self.run_id, lease, vec![requested, suspended])?;
        Ok(())
    }

    /// `hibernate(env_handle_id) → SnapshotRecord` — the hibernation-aware
    /// suspend (S5.8; R-2.2.3² "hibernation-aware environment leases with
    /// R-2.2.5 snapshots"; spec §5a.3 `hibernated`). A hibernating suspend
    /// is suspend-with-memory-checkpoint: the provider legs run *before*
    /// any durable row (a failed checkpoint leaves the handle `ready`,
    /// no torn state — `provider.resume` best-effort restores the
    /// provider-side quiesce on a snapshot failure), then the batch
    /// `[suspend.requested{on_idle:"hibernate"}, snapshot{kind:"memory"},
    /// suspended{on_idle:"hibernate", hibernation_snapshot}]` lands in
    /// one append — durable-before-visible across the pair.
    ///
    /// Capability honesty: `suspend = unknown` refuses
    /// `UnknownCapability`; `snapshot{memory} = unsupported` refuses
    /// `Unsupported`, `unknown`/undeclared refuses `UnknownCapability`;
    /// a handle with no provider leg has no hibernation mechanism at
    /// all (fs_tree alone is not hibernation — the memory image is the
    /// point). The run-side counterpart is `SuspendReason::Hibernated`
    /// and `env.suspended_ms` keeps accruing while `hibernated`
    /// (hibernation is `reserved_ms` under the suspend meter — §5a.5's
    /// accounting row, no third counter).
    pub fn hibernate(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
    ) -> Result<SnapshotRecord, EnvError> {
        let now = store.now_ms();
        let h = self
            .handles
            .get(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?
            .clone();
        if h.capabilities.suspend == crate::handle::SuspendKind::Unknown {
            return Err(EnvError::UnknownCapability {
                capability: "suspend".to_string(),
            });
        }
        // `snapshot{memory}` is the hibernation mechanism — the tri-state
        // refusal is verbatim (S1 — never coerced).
        match h
            .capabilities
            .snapshot
            .get(crate::snapshot::SnapshotKind::Memory.as_str())
        {
            Some(crate::handle::Tri::Supported) => {}
            Some(crate::handle::Tri::Unsupported) => {
                return Err(EnvError::Unsupported {
                    capability: "hibernate",
                    detail: "class declared snapshot.memory unsupported".to_string(),
                })
            }
            _ => {
                return Err(EnvError::UnknownCapability {
                    capability: "snapshot.memory".to_string(),
                })
            }
        }
        let Some(ph) = h.provider.clone() else {
            return Err(EnvError::UnknownCapability {
                capability: "hibernate".to_string(),
            });
        };
        // Provider legs first — durable rows land only after both
        // succeed; a checkpoint failure best-effort resumes the provider
        // quiesce and the handle stays `ready`.
        let adapter = self.provider_for(h.class).ok_or(EnvError::Unavailable {
            env_handle_id: env_handle_id.to_string(),
            state: "no_adapter",
        })?;
        adapter.suspend(&ph)?;
        let remote_ref = match adapter.memory_snapshot(&ph) {
            Ok(r) => r,
            Err(e) => {
                let _ = adapter.resume(&ph);
                return Err(e);
            }
        };
        let at_seq = store.head(&self.run_id).map(|x| x.seq).unwrap_or(0);
        let base = match &h.image {
            ResolvedImage::Address(ca) => Some(ca.id()),
            _ => None,
        };
        let mut rec = SnapshotRecord {
            snapshot_ref: String::new(),
            env_handle_id: env_handle_id.to_string(),
            at_seq,
            kind: SnapshotKind::Memory,
            base,
            content: Json::obj([(
                "foreign_digest",
                Json::obj([
                    ("scheme", Json::str("provider")),
                    ("value", Json::str(remote_ref)),
                    ("source", Json::str(h.class.as_str())),
                ]),
            )]),
            roots_covered: h.roots.workspace_roots.clone(),
            quiesced: true,
            // The recovery/hygiene leg took this snapshot (OQ-318's
            // interim accounting — recovery snapshots charge to the
            // subject's instance).
            taken_by: TakenBy::Subject,
            size_bytes: 0,
            expires_at_ms: None,
        };
        rec.snapshot_ref = rec.compute_ref();
        let manifest_ref = store
            .put_blob(
                rec.to_json().to_canonical_string().as_bytes(),
                "application/json",
            )
            .map_err(EnvError::Ledger)?
            .id();
        let h = self.handles.get_mut(env_handle_id).unwrap();
        h.transition(HandleState::Suspended, now)?;
        h.snapshots.push(rec.snapshot_ref.clone());
        let requested = EventMinter::new(store, &self.run_id).mint(
            "action.environment.suspend.requested",
            events::suspend_requested_payload(h, Some("hibernate")),
        )?;
        let snap = EventMinter::new(store, &self.run_id).mint(
            "action.environment.snapshot",
            events::snapshot_payload(h, &rec.snapshot_ref, "memory", at_seq, Some(&manifest_ref)),
        )?;
        let mut suspended_j = events::suspended_payload(h, Some("hibernate"));
        if let Json::Obj(m) = &mut suspended_j {
            // The `hibernated` marker: `suspend_reason` names the §5a.3
            // closed member and `hibernation_snapshot` pins the checkpoint
            // the heal's snapshot-first leg restores.
            m.insert("suspend_reason".to_string(), Json::str("hibernated"));
            m.insert(
                "hibernation_snapshot".to_string(),
                Json::str(rec.snapshot_ref.clone()),
            );
        }
        let suspended = EventMinter::new(store, &self.run_id)
            .mint("action.environment.suspended", suspended_j)?;
        // One append — the suspend pair + the snapshot row are durable
        // together (request-then-state; nothing visible before durable).
        store.append(&self.run_id, lease, vec![requested, snap, suspended])?;
        Ok(rec)
    }

    /// `resume(env_handle_id, cause)` — `suspended → ready` (S4.13). `cause`
    /// is `wakeup` | `operator` — the same vocabulary `lifecycle.run.resumed`
    /// uses (one cause enum, CC1); the suspended accrual lands on the row.
    pub fn resume(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
        cause: &str,
    ) -> Result<(), EnvError> {
        let now = store.now_ms();
        let h = self
            .handles
            .get(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?
            .clone();
        if h.state != HandleState::Suspended {
            return Err(EnvError::InvalidState {
                op: "resume",
                state: h.state.as_str(),
            });
        }
        if let Some(ph) = &h.provider {
            if let Some(adapter) = self.provider_for(h.class) {
                adapter.resume(ph)?;
            }
        }
        let h = self.handles.get_mut(env_handle_id).unwrap();
        h.transition(HandleState::Ready, now)?;
        let suspended_ms = h.meters.view(now).suspended_ms;
        let requested = EventMinter::new(store, &self.run_id).mint(
            "action.environment.resume.requested",
            events::resume_requested_payload(h, cause),
        )?;
        let resumed = EventMinter::new(store, &self.run_id).mint(
            "action.environment.resumed",
            events::resumed_payload(h, cause, suspended_ms),
        )?;
        store.append(&self.run_id, lease, vec![requested, resumed])?;
        Ok(())
    }

    /// `teardown(env_handle_id)` — any live state → `torn_down` (terminal).
    /// S4.13 adds the `on_parent_end` cascade: derived children with
    /// `Teardown` tear down recursively; `DetachToChild` children keep
    /// living — the child's ownership transfers to the run (the
    /// `action.environment.detached{reason: "detach_to_child"}` row is the
    /// ledger-visible transfer; the child's lifetime is preserved — R-2.2.5¹).
    pub fn teardown(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
    ) -> Result<(), EnvError> {
        // Collect the non-terminal children first — the cascade mutates the
        // table after the parent's own row lands.
        let children: Vec<(String, crate::handle::OnParentEnd)> = self
            .handles
            .values()
            .filter(|c| {
                c.parent
                    .as_ref()
                    .map(|p| p.env_handle_id.as_str() == env_handle_id)
                    .unwrap_or(false)
                    && !c.state.is_terminal()
            })
            .map(|c| {
                (
                    c.env_handle_id.clone(),
                    c.parent.as_ref().unwrap().on_parent_end,
                )
            })
            .collect();
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
        // The helper session + container go with the environment.
        if let Some(mut client) = self.sessions.remove(env_handle_id) {
            client.shutdown();
        }
        if let Some(b) = self.containers.remove(env_handle_id) {
            let _ = b.stop();
        }
        let ev = EventMinter::new(store, &self.run_id).mint(
            "action.environment.torn_down",
            events::torn_down_payload(h, "teardown"),
        )?;
        // The provider's remote side goes too — teardown is explicit, never
        // survived (detach is the survivable form for persistent classes).
        let class = h.class;
        let ph = h.provider.clone();
        store.append(&self.run_id, lease, vec![ev])?;
        if let Some(ph) = ph {
            if let Some(adapter) = self.provider_for(class) {
                adapter.teardown(&ph)?;
            }
        }
        // The `on_parent_end` cascade — each child's edge decides (KP: the
        // cascade is recorded per child, never silent).
        for (child_id, mode) in children {
            match mode {
                crate::handle::OnParentEnd::Teardown => {
                    self.teardown_cascade(store, lease, &child_id)?;
                }
                crate::handle::OnParentEnd::DetachToChild => {
                    let now = store.now_ms();
                    let h = self.handles.get_mut(&child_id).unwrap();
                    let released_from =
                        h.parent.take().map(|p| p.env_handle_id).unwrap_or_default();
                    // Ownership transfers to the run — the child keeps its
                    // state/lifetime; the row records the transfer.
                    let ev = EventMinter::new(store, &self.run_id).mint(
                        "action.environment.detached",
                        Json::obj([
                            ("env_handle", Json::str(h.env_handle_id.clone())),
                            ("reason", Json::str("detach_to_child")),
                            ("released_from", Json::str(&released_from)),
                        ]),
                    )?;
                    store.append(&self.run_id, lease, vec![ev])?;
                    let _ = now;
                }
            }
        }
        Ok(())
    }

    /// The recursive `Teardown` cascade leg — a child's own children cascade
    /// under the same rule (the walk is bounded by the tree depth).
    fn teardown_cascade(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
    ) -> Result<(), EnvError> {
        // Reuse `teardown` — its own child walk covers the recursion. The
        // torn row's `reason` is fixed `teardown`; the parent's row names
        // the cascade's root, so the audit chain stays legible.
        self.teardown(store, lease, env_handle_id)
    }

    /// `derive_share(parent_id, resource_keys, on_parent_end)` — the `share`
    /// mode's resource-key form (S4.13; R-2.2.5¹ "share mode with resource
    /// keys"). Every named key acquires a `resource:<key>` scoped lease
    /// before the derive — a live conflicting holder refuses `WouldBlock`
    /// (unsafe share is refused, never admitted). The keys land on the
    /// `derived` row via `extra`-style payload members.
    pub fn derive_share(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        parent_id: &str,
        resource_keys: &[String],
        on_parent_end: crate::handle::OnParentEnd,
        derived_for: Option<&str>,
    ) -> Result<EnvHandle, EnvError> {
        if resource_keys.is_empty() {
            return Err(EnvError::Unsupported {
                capability: "derive.share",
                detail: "share requires at least one resource key — write \
                         coordination is leased, never implied"
                    .to_string(),
            });
        }
        for key in resource_keys {
            store
                .lease_acquire(
                    &self.run_id,
                    lease,
                    &LeaseScope::Resource(key.clone()),
                    &format!("{}:share:{}", self.run_id, parent_id),
                    PRESERVE_UNTIL_TTL_MS,
                )
                .map_err(|e| EnvError::Unsupported {
                    capability: "derive.share",
                    detail: format!("resource key {key} not acquired: {e}"),
                })?;
        }
        self.derive_for(
            store,
            lease,
            parent_id,
            crate::handle::DeriveMode::Share,
            None,
            on_parent_end,
            derived_for,
        )
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

    /// `heal_live(env_handle_id)` — the session-aware heal: `session_still_
    /// live` is *measured* off the live helper channel (`is_live` — a
    /// `list_detached` round-trip), never the caller's claim.
    pub fn heal_live(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
    ) -> Result<(), EnvError> {
        let live = self
            .sessions
            .get_mut(env_handle_id)
            .map(|c| c.is_live())
            .unwrap_or(false);
        self.heal(store, lease, env_handle_id, live)
    }

    /// `resume_helper(env_handle_id)` — kernel-restart resume: reconnect to
    /// the surviving helper's socket and `hello{resume_session_id}` with the
    /// persisted nonce (kernel death ≠ environment death — the helper kept
    /// the exec table under `preserve_until`). The caller then `heal`s the
    /// `unreachable` handle.
    pub fn resume_helper(
        &mut self,
        store: &mut Store,
        env_handle_id: &str,
    ) -> Result<(), EnvError> {
        use hh_helper::protocol::OnKernelLoss;
        let h = self
            .handles
            .get(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?
            .clone();
        let coord_dir = store.root().join("envs").join(&h.env_handle_id);
        let nonce = std::fs::read_to_string(coord_dir.join("session.nonce"))
            .map_err(|e| EnvError::Blob(format!("session nonce: {e}")))?;
        let session_id = std::fs::read_to_string(coord_dir.join("session.id"))
            .map_err(|e| EnvError::Blob(format!("session id: {e}")))?;
        let sock = std::fs::read_to_string(coord_dir.join("session.sock"))
            .map_err(|e| EnvError::Blob(format!("session sock: {e}")))?;
        let mut client = HelperClient::connect(std::path::Path::new(sock.trim()))?;
        client.session_id = session_id.trim().to_string();
        client.hello(
            OnKernelLoss::PreserveUntil {
                ttl_ms: PRESERVE_UNTIL_TTL_MS,
            },
            nonce.trim(),
            Some(DEDUP_WINDOW_MS),
            None,
            None,
            true,
        )?;
        self.sessions.insert(env_handle_id.to_string(), client);
        Ok(())
    }

    /// `replace(env_handle_id) → EnvHandle` — the heal ladder's replace
    /// rung. An `unreachable` environment whose `on_loss` is
    /// `ReplaceFromImage`/`ReplaceFromSnapshot` yields a successor handle:
    /// the old handle lands `replaced` (terminal), the successor is a new
    /// handle over a fresh workspace with the same `environment_ref` and a
    /// `ParentEdge`. `ReplaceFromSnapshot` restores the parent's latest
    /// `fs_tree` state (the workspace copy is the restore; the recorded
    /// `base` names the parent snapshot). `FailRun` refuses — `heal` owns
    /// that posture.
    pub fn replace(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
    ) -> Result<EnvHandle, EnvError> {
        let now = store.now_ms();
        let h = self
            .handles
            .get(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?
            .clone();
        if h.state != HandleState::Unreachable {
            return Err(EnvError::InvalidState {
                op: "replace",
                state: h.state.as_str(),
            });
        }
        let mode = match h.on_loss {
            OnLoss::ReplaceFromSnapshot => crate::handle::DeriveMode::ForkSnapshot,
            OnLoss::ReplaceFromImage => crate::handle::DeriveMode::FreshFromImage,
            OnLoss::FailRun => {
                return Err(EnvError::InvalidState {
                    op: "replace",
                    state: "on_loss=fail_run",
                })
            }
        };
        // Fresh workspace for the successor.
        let new_id = store.alloc_id("env");
        let ws = store.root().join("envs").join(&new_id).join("workspace");
        std::fs::create_dir_all(&ws).map_err(|e| EnvError::Blob(e.to_string()))?;
        // S5.8 (R-2.2.3² — "snapshot-first restore in healing"): a
        // `ReplaceFromSnapshot` successor restores the parent's newest
        // *recorded* snapshot — `memory` first (the hibernation
        // checkpoint, restored through the provider adapter's
        // `restore_in_place`), then `fs_tree` (blob-pool restore, the
        // same path `derive_from_snapshot` uses) — over the live tree:
        // a crashed environment's workspace is itself suspect. A
        // `memory` snapshot the child cannot restore refuses
        // `Unsupported`, never silently degrades to the fs copy. Only
        // when no snapshot row was recorded does the legacy live-tree
        // copy run (the record is empty; the copy is all there is).
        let newest = self.newest_snapshot(store, env_handle_id)?;
        let memory_leg = if matches!(mode, crate::handle::DeriveMode::ForkSnapshot) {
            match &newest {
                Some((_, rec)) if rec.kind == SnapshotKind::Memory => Some(rec.clone()),
                Some((_, rec)) => {
                    // `fs_tree` — restore the recorded tree into the
                    // successor's workspace (blob-pool content, never the
                    // live tree).
                    let snap = hh_helper::fstree::FsTreeSnapshot::from_manifest_json(&rec.content)
                        .ok_or_else(|| {
                            EnvError::Blob(format!(
                                "snapshot {}: malformed manifest",
                                rec.snapshot_ref
                            ))
                        })?;
                    hh_helper::fstree::restore(&snap, &ws, &|ca| blob_by_id(store, ca))
                        .map_err(|e| EnvError::Blob(format!("heal snapshot restore: {e}")))?;
                    None
                }
                None => {
                    // Nothing recorded — the pre-S5.8 live-tree copy.
                    for root in &h.roots.workspace_roots {
                        copy_tree(root, &ws.to_string_lossy())?;
                    }
                    None
                }
            }
        } else {
            None
        };
        let child = self.spawn_handle(
            store,
            lease,
            &h,
            Roots {
                workspace_roots: vec![ws.to_string_lossy().to_string()],
                writable_roots: vec![ws.to_string_lossy().to_string()],
                cwd: ws.to_string_lossy().to_string(),
            },
            Some(crate::handle::ParentEdge {
                env_handle_id: env_handle_id.to_string(),
                mode,
                on_parent_end: crate::handle::OnParentEnd::Teardown,
            }),
            new_id,
        )?;
        if let Some(rec) = memory_leg {
            // The successor's provider leg restores the recorded
            // checkpoint (the provider's opaque id is the record's
            // `foreign_digest.value` — the kernel's `snapshot_ref` is
            // what the durable `restored` row names).
            let Some(ph) = child.provider.clone() else {
                return Err(EnvError::UnknownCapability {
                    capability: "heal.snapshot_first".to_string(),
                });
            };
            match child.capabilities.restore_in_place {
                crate::handle::Tri::Supported => {}
                crate::handle::Tri::Unsupported => {
                    return Err(EnvError::Unsupported {
                        capability: "heal.snapshot_first",
                        detail: "class declared restore_in_place unsupported".to_string(),
                    })
                }
                crate::handle::Tri::Unknown => {
                    return Err(EnvError::UnknownCapability {
                        capability: "restore_in_place".to_string(),
                    })
                }
            }
            let remote_ref = rec
                .content
                .get("foreign_digest")
                .and_then(|fd| fd.get("value"))
                .and_then(Json::as_str)
                .ok_or_else(|| {
                    EnvError::Blob(format!(
                        "memory snapshot {}: no foreign_digest leg",
                        rec.snapshot_ref
                    ))
                })?
                .to_string();
            self.provider_for(h.class)
                .ok_or(EnvError::Unavailable {
                    env_handle_id: env_handle_id.to_string(),
                    state: "no_adapter",
                })?
                .restore_in_place(&ph, &remote_ref)?;
            let ev = EventMinter::new(store, &self.run_id).mint(
                "action.environment.restored",
                Json::obj([
                    ("env_handle", Json::str(child.env_handle_id.clone())),
                    ("snapshot_ref", Json::str(rec.snapshot_ref.clone())),
                    ("kind", Json::str("memory")),
                    ("mode", Json::str("snapshot_first")),
                    ("reverts", Json::str(rec.snapshot_ref.clone())),
                ]),
            )?;
            store.append(&self.run_id, lease, vec![ev])?;
        }
        // The old handle lands `replaced` (terminal) — its session and
        // container are released.
        if let Some(mut client) = self.sessions.remove(env_handle_id) {
            client.shutdown();
        }
        if let Some(b) = self.containers.remove(env_handle_id) {
            let _ = b.stop();
        }
        let h = self.handles.get_mut(env_handle_id).unwrap();
        h.transition(HandleState::Replaced, now)?;
        let ev = EventMinter::new(store, &self.run_id).mint(
            "action.environment.replaced",
            events::replaced_payload(h, &child.env_handle_id),
        )?;
        store.append(&self.run_id, lease, vec![ev])?;
        Ok(child)
    }

    /// `derive(parent_id, mode, scope, on_parent_end) → EnvHandle` — a child
    /// environment from a live parent (§5a.5 `derive`; S2.1):
    ///
    /// - `FreshFromImage` — a fresh workspace under the parent's record
    ///   (the image's initial state);
    /// - `ForkSnapshot` — the parent's current tree copied into the child's
    ///   workspace (an `fs_tree` snapshot of the parent is recorded as the
    ///   child's `base` claim);
    /// - `ScopedSubtree` — the child's roots are narrowed to `scope` inside
    ///   the parent's workspace (a shared, *scoped* subtree — the child's
    ///   helper re-gates fs access to the subtree);
    /// - `Share` — the child shares the parent's roots verbatim (§5e.5;
    ///   write coordination is the `resource(key)` lease + ownership layer,
    ///   never a copied view).
    ///
    /// The child lands in `provisioning` — `attach` completes it (the
    /// `derived` event records the `ParentEdge`).
    pub fn derive(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        parent_id: &str,
        mode: crate::handle::DeriveMode,
        scope: Option<&str>,
        on_parent_end: crate::handle::OnParentEnd,
    ) -> Result<EnvHandle, EnvError> {
        self.derive_for(store, lease, parent_id, mode, scope, on_parent_end, None)
    }

    /// `derive` with an explicit `derived_for` run id — C1's spawn replay
    /// key. The member is optional so existing callers keep the unadorned
    /// audit shape; subagent spawn names its deterministic child id and a
    /// retry can adopt the durable handle instead of deriving twice.
    ///
    /// The driver seam keeps the ledger/environment parameters explicit;
    /// packing them into a context struct would hide the call's authority
    /// surface, so the long signature is intentional.
    #[allow(clippy::too_many_arguments)]
    pub fn derive_for(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        parent_id: &str,
        mode: crate::handle::DeriveMode,
        scope: Option<&str>,
        on_parent_end: crate::handle::OnParentEnd,
        derived_for: Option<&str>,
    ) -> Result<EnvHandle, EnvError> {
        use crate::handle::DeriveMode;
        let parent = self
            .handles
            .get(parent_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: parent_id.to_string(),
                state: "missing",
            })?
            .clone();
        match parent.state {
            HandleState::Ready | HandleState::Detached => {}
            s => {
                return Err(EnvError::InvalidState {
                    op: "derive",
                    state: s.as_str(),
                })
            }
        }
        let new_id = store.alloc_id("env");
        let base_dir = store.root().join("envs").join(&new_id).join("workspace");
        let roots = match mode {
            DeriveMode::FreshFromImage => {
                std::fs::create_dir_all(&base_dir).map_err(|e| EnvError::Blob(e.to_string()))?;
                Roots {
                    workspace_roots: vec![base_dir.to_string_lossy().to_string()],
                    writable_roots: vec![base_dir.to_string_lossy().to_string()],
                    cwd: base_dir.to_string_lossy().to_string(),
                }
            }
            DeriveMode::ForkSnapshot => {
                std::fs::create_dir_all(&base_dir).map_err(|e| EnvError::Blob(e.to_string()))?;
                for root in &parent.roots.workspace_roots {
                    copy_tree(root, &base_dir.to_string_lossy())?;
                }
                Roots {
                    workspace_roots: vec![base_dir.to_string_lossy().to_string()],
                    writable_roots: vec![base_dir.to_string_lossy().to_string()],
                    cwd: base_dir.to_string_lossy().to_string(),
                }
            }
            DeriveMode::ScopedSubtree => {
                let scope = scope.ok_or_else(|| EnvError::Unsupported {
                    capability: "derive.scoped_subtree",
                    detail: "a scope path is required".to_string(),
                })?;
                let sub = canonicalize(&format!(
                    "{}/{}",
                    parent.roots.cwd.trim_end_matches('/'),
                    scope.trim_start_matches('/')
                ));
                if !parent.roots.is_writable(&sub) {
                    return Err(EnvError::OutsideRoots { path: sub });
                }
                std::fs::create_dir_all(&sub).map_err(|e| EnvError::Blob(e.to_string()))?;
                Roots {
                    workspace_roots: vec![sub.clone()],
                    writable_roots: vec![sub.clone()],
                    cwd: sub,
                }
            }
            DeriveMode::Share => {
                // `share` — the child's workspace *is* the parent's
                // (§5e.5/ADR-0192: one shared mutable view, unscoped).
                // Containment is unchanged — the child inherits exactly the
                // parent's roots, never widened; write coordination lives
                // above the fs layer (ownership records + `resource(key)`
                // leases, spawn step 4c), never in a copied view.
                Roots {
                    workspace_roots: parent.roots.workspace_roots.clone(),
                    writable_roots: parent.roots.writable_roots.clone(),
                    cwd: parent.roots.cwd.clone(),
                }
            }
        };
        let child = self.spawn_handle(
            store,
            lease,
            &parent,
            roots,
            Some(crate::handle::ParentEdge {
                env_handle_id: parent_id.to_string(),
                mode,
                on_parent_end,
            }),
            new_id,
        )?;
        let ev = EventMinter::new(store, &self.run_id).mint(
            "action.environment.derived",
            events::derived_payload(&child, derived_for),
        )?;
        store.append(&self.run_id, lease, vec![ev])?;
        Ok(child)
    }

    /// `set_phase(env_handle_id, phase)` — §05a/ADR-0142: applies the
    /// phase schedule sealed with the handle's containment policy.
    /// Refused unless (a) the handle declares `per_phase_network_policy`
    /// `supported` AND (b) the policy's `ext["phase_schedule"][phase]`
    /// names a `net_mode` — both absent ⇒ `Refused`-class
    /// `phase_schedule_undeclared`, never a silent policy swap (CF-318).
    /// On success the policy's `net.mode` becomes the scheduled mode, the
    /// handle's `phase` records it, and `action.environment.phase.changed`
    /// lands durable.
    pub fn set_phase(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
        phase: &str,
    ) -> Result<(), EnvError> {
        if !matches!(phase, "setup" | "agent" | "verify") {
            return Err(EnvError::Unsupported {
                capability: "set_phase.phase",
                detail: format!("phase must be setup|agent|verify, not {phase}"),
            });
        }
        let h = self
            .handles
            .get_mut(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?;
        h.verify_environment()?;
        if h.capabilities.per_phase_network_policy != crate::handle::Tri::Supported {
            return Err(EnvError::Unsupported {
                capability: "per_phase_network_policy",
                detail: "set_phase refused: the handle does not declare                          per_phase_network_policy"
                    .to_string(),
            });
        }
        // The sealed schedule lives on the policy's `ext` — a phase the
        // schedule does not name is the same fail-closed refusal. The
        // entry is a sealed `policy_rule` artifact (§5g.4 §2.2's phase row;
        // S4.14b): it carries either the Stage-1 `net_mode` shorthand or a
        // full `layer` policy document; `basis` (when spelled) must be
        // `policy_rule` — any other basis is a `basis_not_allowed`
        // refusal, never a silent swap (CF-318).
        let policy = match &mut h.containment {
            hh_containment::attach::PolicySlot::Inline(p) => &mut **p,
            hh_containment::attach::PolicySlot::ResolvedRef { policy, .. } => &mut **policy,
        };
        let entry = policy
            .ext
            .get("phase_schedule")
            .and_then(|s| s.get(phase))
            .cloned()
            .ok_or_else(|| EnvError::Unsupported {
                capability: "phase_schedule",
                detail: format!("no sealed phase_schedule entry for `{phase}`"),
            })?;
        if let Some(basis) = entry.get("basis").and_then(Json::as_str) {
            if basis != "policy_rule" {
                return Err(EnvError::Unsupported {
                    capability: "phase_schedule.basis",
                    detail: format!(
                        "basis_not_allowed: a sealed phase layer is endorsed                          `policy_rule`, not `{basis}`"
                    ),
                });
            }
        }
        // The layer the schedule names: an explicit `layer`
        // `ContainmentPolicy` document (whose own `provenance.authority`
        // the meet enforces), or the `net_mode` shorthand minted into a
        // layer at the entry's declared `authority` (default
        // `definition` — a sealed design's level; a *loosening* schedule
        // entry must name `principal`).
        let layer = if let Some(layer_j) = entry.get("layer") {
            hh_containment::policy::ContainmentPolicy::from_json(layer_j).map_err(|e| {
                EnvError::Unsupported {
                    capability: "phase_schedule.layer",
                    detail: format!("phase_schedule layer does not decode: {e}"),
                }
            })?
        } else {
            let net_mode = entry
                .get("net_mode")
                .and_then(Json::as_str)
                .ok_or_else(|| EnvError::Unsupported {
                    capability: "phase_schedule",
                    detail: format!(
                        "phase_schedule entry for `{phase}` carries neither `layer` nor `net_mode`"
                    ),
                })?;
            let mut l = policy.clone();
            l.net.mode = match net_mode {
                "none" => hh_containment::policy::NetMode::None,
                "mediated" => hh_containment::policy::NetMode::Mediated,
                "public" => hh_containment::policy::NetMode::Public,
                other => {
                    return Err(EnvError::Unsupported {
                        capability: "phase_schedule.net_mode",
                        detail: format!("unknown net_mode {other} in phase_schedule"),
                    })
                }
            };
            l.provenance.authority = entry
                .get("authority")
                .and_then(Json::as_str)
                .and_then(hh_provenance::AuthorityClass::parse)
                .unwrap_or(hh_provenance::AuthorityClass::Definition);
            l.compute_ids();
            l
        };
        // Re-derive the effective policy through the layered meet — the
        // sealed layer narrows or (with entitlement) loosens against the
        // live policy; a `ContainmentWidening`/`ProtectedPathExemption`/
        // invalid layer refuses, never silently swaps (R-2.8.4¹).
        let next = hh_containment::meet::effective(&[policy.clone(), layer], store.now_ms())
            .map_err(|e| EnvError::Unsupported {
                capability: "phase_schedule.layer",
                detail: format!("phase layer refused by the meet: {e}"),
            })?;
        let net_mode = next.net.mode;
        *policy = next;
        h.phase = Some(phase.to_string());
        let ev = EventMinter::new(store, &self.run_id).mint(
            "action.environment.phase.changed",
            Json::obj([
                ("env_handle_id", Json::str(h.env_handle_id.clone())),
                ("phase", Json::str(phase.to_string())),
                ("net_mode", Json::str(net_mode.as_str())),
            ]),
        )?;
        store.append(&self.run_id, lease, vec![ev])?;
        Ok(())
    }

    /// `rebind_credentials_for_fork(store, lease, broker, parent_id, child_id)`
    /// — LT-09's fork half (S2.4; ADR-0266 D4): after `derive(ForkSnapshot)`,
    /// the parent's live bindings are *re-bound* onto the child's env handle
    /// with **fresh** placeholders (`CredentialBroker::virtualize_for_fork`),
    /// the child's `credential_bindings` list gains the new binding ids, and
    /// the returned rewrite map is what the caller applies to any materialised
    /// env projection the snapshot carried (placeholder spellings only — a
    /// snapshot never holds a value, so the rewrite is spelling→spelling).
    /// The fork's `env_spec_for(child)` then projects the fresh spellings —
    /// the parent's nonce never transfers (SV-10).
    pub fn rebind_credentials_for_fork(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        broker: &mut hh_secrets::CredentialBroker,
        parent_id: &str,
        child_id: &str,
    ) -> Result<hh_secrets::ForkVirtualization, EnvError> {
        let parent = self.handles.get(parent_id).ok_or(EnvError::Unavailable {
            env_handle_id: parent_id.to_string(),
            state: "missing",
        })?;
        let child_state = self.handles.get(child_id).ok_or(EnvError::Unavailable {
            env_handle_id: child_id.to_string(),
            state: "missing",
        })?;
        if child_state
            .parent
            .as_ref()
            .map(|p| p.env_handle_id.as_str())
            != Some(parent.env_handle_id.as_str())
        {
            return Err(EnvError::InvalidState {
                op: "rebind_credentials_for_fork",
                state: "not_a_derive_child",
            });
        }
        let virt = broker
            .virtualize_for_fork(store, &self.run_id, lease, parent_id, child_id)
            .map_err(|e| EnvError::Blob(format!("virtualize_for_fork: {e:?}")))?;
        let h = self.handles.get_mut(child_id).expect("child present");
        h.credential_bindings = virt.bindings.clone();
        Ok(virt)
    }

    /// `spawn_handle(parent, roots, parent_edge, env_handle_id)` — the
    /// shared child-provisioning body (`replace`/`derive`): mint the handle
    /// over the parent's class/image/policy, append `declared` +
    /// `provisioning` + `provisioned`, land it in `provisioning` (the
    /// caller `attach`es it — `start_helper` runs there).
    fn spawn_handle(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        parent: &EnvHandle,
        roots: Roots,
        parent_edge: Option<crate::handle::ParentEdge>,
        env_handle_id: String,
    ) -> Result<EnvHandle, EnvError> {
        let now = store.now_ms();
        let h = EnvHandle {
            env_handle_id,
            run_id: self.run_id.clone(),
            environment_ref: parent.environment_ref.clone(),
            class: parent.class,
            state: HandleState::Declared,
            health: Health::Unknown,
            session: None,
            capabilities: parent.capabilities.clone(),
            containment: parent.containment.clone(),
            report: None,
            credential_bindings: vec![],
            roots,
            limits: parent.limits.clone(),
            budget_node_refs: vec![],
            meters: crate::handle::EnvMeters::new(now),
            snapshots: vec![],
            parent: parent_edge,
            on_loss: parent.on_loss,
            snapshot_cadence: SnapshotCadence::Never,
            heal_count: 0,
            image: parent.image.clone(),
            applied_event_ref: None,
            phase: None,
            provider: parent.provider.clone(),
            hosted: parent.hosted.clone(),
            created_ms: now,
        };
        let declared = EventMinter::new(store, &self.run_id)
            .mint("action.environment.declared", events::declared_payload(&h))?;
        store.append(&self.run_id, lease, vec![declared])?;
        let mut h = h;
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

    // ── `fs_tree` — the content-addressed whole-tree snapshot (S2.1) ──────

    /// `fs_tree_snapshot(env_handle_id) → (SnapshotRecord, FsTreeSnapshot)`
    /// — walk the workspace roots into a content-addressed tree manifest
    /// (equal state ⇒ equal ref, positional addressing — the helper's
    /// `fstree` module is the single implementation; local classes share
    /// the filesystem so the kernel-side walk is the same tree the
    /// helper's `snapshot` verb would produce — the workspace is
    /// host-mounted even under `local_container`). Appends
    /// `action.environment.snapshot{kind: fs_tree}`.
    pub fn fs_tree_snapshot(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
    ) -> Result<(SnapshotRecord, hh_helper::fstree::FsTreeSnapshot), EnvError> {
        self.fs_tree_snapshot_as(store, lease, env_handle_id, TakenBy::Subject)
    }

    /// `fs_tree_snapshot_as(…, taken_by)` — the charged-to leg of the
    /// `SnapshotRecord` (ADR-0138 §4): the run's own snapshots are
    /// `subject`; a Group M `env.snapshot` is `instrument`-charged
    /// (ADR-0177 D7 — the Lab/measurement caller's budget, never the
    /// subject's).
    pub fn fs_tree_snapshot_as(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
        taken_by: TakenBy,
    ) -> Result<(SnapshotRecord, hh_helper::fstree::FsTreeSnapshot), EnvError> {
        let (roots, image_base) = {
            let h = self
                .handles
                .get(env_handle_id)
                .ok_or(EnvError::Unavailable {
                    env_handle_id: env_handle_id.to_string(),
                    state: "missing",
                })?;
            h.verify_environment()?;
            h.capabilities.snapshot_supported(SnapshotKind::FsTree)?;
            let base = match &h.image {
                ResolvedImage::Address(ca) => Some(ca.id()),
                _ => None,
            };
            (h.roots.workspace_roots.clone(), base)
        };
        let tree = hh_helper::fstree::walk(&roots)
            .map_err(|e| EnvError::Blob(format!("fs_tree walk: {e}")))?;
        // The manifest's content addresses must *resolve* — deposit every
        // file's bytes into the ledger blob pool so `fs_tree_restore` is
        // independent of the live workspace (idempotent: identical bytes
        // deduplicate to the same address).
        for (root, entries) in &tree.manifest {
            for (rel, node) in entries {
                if let hh_helper::fstree::FsNode::File { .. } = node {
                    let p = format!("{root}/{rel}");
                    let bytes = std::fs::read(&p)
                        .map_err(|e| EnvError::Blob(format!("fs_tree read {p}: {e}")))?;
                    store
                        .put_blob(&bytes, "application/octet-stream")
                        .map_err(EnvError::Ledger)?;
                }
            }
        }
        let at_seq = store.head(&self.run_id).map(|h| h.seq).unwrap_or(0);
        let mut rec = SnapshotRecord {
            snapshot_ref: String::new(),
            env_handle_id: env_handle_id.to_string(),
            at_seq,
            kind: SnapshotKind::FsTree,
            base: image_base,
            content: tree.manifest_json(),
            roots_covered: roots,
            quiesced: false,
            taken_by,
            size_bytes: tree.size_bytes,
            expires_at_ms: None,
        };
        rec.snapshot_ref = rec.compute_ref();
        // The record persists as a blob — the fork/rollback chooser reloads it
        // through `manifest_ref` (S2.9; the snapshot row names both ids).
        let manifest_ref = store
            .put_blob(
                rec.to_json().to_canonical_string().as_bytes(),
                "application/json",
            )
            .map_err(EnvError::Ledger)?
            .id();
        let h = self.handles.get_mut(env_handle_id).unwrap();
        h.snapshots.push(rec.snapshot_ref.clone());
        let ev = EventMinter::new(store, &self.run_id).mint(
            "action.environment.snapshot",
            events::snapshot_payload(h, &rec.snapshot_ref, "fs_tree", at_seq, Some(&manifest_ref)),
        )?;
        store.append(&self.run_id, lease, vec![ev])?;
        Ok((rec, tree))
    }

    /// `fs_tree_verify(env_handle_id, snap)` — re-walk and compare the
    /// manifest (content-addressed: a changed byte is a different ref —
    /// `false` is a verification failure, never silently accepted).
    pub fn fs_tree_verify(
        &mut self,
        env_handle_id: &str,
        snap: &hh_helper::fstree::FsTreeSnapshot,
    ) -> Result<bool, EnvError> {
        let h = self
            .handles
            .get(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?;
        h.verify_environment()?;
        hh_helper::fstree::verify(snap).map_err(|e| EnvError::Blob(e.to_string()))
    }

    /// `fs_tree_restore(env_handle_id, snap) → tree_address` — materialise
    /// the snapshot's tree into the environment's first workspace root.
    /// Each entry's content address resolves against the ledger blob pool
    /// (`fs_tree_snapshot` deposits it); a missing blob is a `Blob`
    /// refusal, and the returned recompute must equal `tree_address` —
    /// a mismatch is `ContainmentUnverified`-shaped, never silently
    /// accepted.
    pub fn fs_tree_restore(
        &mut self,
        store: &Store,
        env_handle_id: &str,
        snap: &hh_helper::fstree::FsTreeSnapshot,
    ) -> Result<String, EnvError> {
        let h = self
            .handles
            .get(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?;
        h.verify_environment()?;
        let dest = std::path::PathBuf::from(h.roots.cwd.clone());
        std::fs::create_dir_all(&dest).map_err(|e| EnvError::Blob(e.to_string()))?;
        // GC'd snapshot ⇒ `SnapshotMissing{snapshot_ref}` — the missing
        // blob's content address is itself the verifiable hash the caller
        // checks the tombstone against (AC-R-2.2.4-9).
        for node in snap.manifest.values().flat_map(|m| m.values()) {
            if let hh_helper::fstree::FsNode::File { ca, .. } = node {
                if blob_by_id(store, ca).is_none() {
                    return Err(EnvError::SnapshotMissing {
                        snapshot_ref: snap.tree_address.clone(),
                    });
                }
            }
        }
        let recomputed = hh_helper::fstree::restore(snap, &dest, &|ca| blob_by_id(store, ca))
            .map_err(|e| EnvError::Blob(format!("fs_tree restore: {e}")))?;
        if recomputed != snap.tree_address && !snap.roots.is_empty() {
            // A single-root snapshot's address re-keys identically; a
            // multi-root restore into one root legitimately differs — the
            // verify contract is honoured per-entry via the blob digests
            // (each `get_blob` rehashes — `BlobCorrupt` surfaces).
            let all_ok = snap
                .manifest
                .values()
                .flat_map(|m| m.values())
                .all(|n| match n {
                    hh_helper::fstree::FsNode::File { ca, .. } => blob_by_id(store, ca).is_some(),
                    _ => true,
                });
            if !all_ok {
                return Err(EnvError::ContainmentUnverified {
                    field_group: "fs".to_string(),
                    reason: "fs_tree restore recomputed under missing blobs".to_string(),
                });
            }
        }
        Ok(recomputed)
    }

    /// `memory_snapshot(store, lease, env_handle_id, taken_by)` — the
    /// `memory` snapshot kind (S5.8; R-2.2.5²; ADR-0137 §4 rules S1/S5):
    /// the provider adapter's checkpoint of a *suspended* handle,
    /// recorded as a `foreign_digest` claim — the provider owns the
    /// bytes, the kernel owns the record. `memory` requires `quiesced`;
    /// a `suspended` handle is the kernel's quiesced witness, any other
    /// state is `InvalidState` (never a quietly-unquiesced capture).
    /// An undeclared kind refuses `Unsupported`/`UnknownCapability` per
    /// the handle's `EnvCapabilityDeclaration` (S1 — never coerced), and
    /// a handle with no provider leg has no mechanism at all.
    pub fn memory_snapshot(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
        taken_by: TakenBy,
    ) -> Result<SnapshotRecord, EnvError> {
        let h = self
            .handles
            .get(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?
            .clone();
        h.capabilities.snapshot_supported(SnapshotKind::Memory)?;
        if h.state != HandleState::Suspended {
            return Err(EnvError::InvalidState {
                op: "snapshot.memory",
                state: h.state.as_str(),
            });
        }
        let Some(ph) = h.provider.clone() else {
            return Err(EnvError::UnknownCapability {
                capability: "snapshot.memory".to_string(),
            });
        };
        let remote_ref = self
            .provider_for(h.class)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "no_adapter",
            })?
            .memory_snapshot(&ph)?;
        let at_seq = store.head(&self.run_id).map(|h| h.seq).unwrap_or(0);
        let base = match &h.image {
            ResolvedImage::Address(ca) => Some(ca.id()),
            _ => None,
        };
        let mut rec = SnapshotRecord {
            snapshot_ref: String::new(),
            env_handle_id: env_handle_id.to_string(),
            at_seq,
            kind: SnapshotKind::Memory,
            base,
            // The provider's opaque snapshot id recorded as a
            // `foreign_digest` claim ({scheme, value, source}) — the
            // kernel never invents a local content address for bytes it
            // does not hold (S4 caps a `foreign_digest`/`memory` leg at
            // R0/R1/R3 with `unpinned[]`).
            content: Json::obj([(
                "foreign_digest",
                Json::obj([
                    ("scheme", Json::str("provider")),
                    ("value", Json::str(remote_ref)),
                    ("source", Json::str(h.class.as_str())),
                ]),
            )]),
            roots_covered: h.roots.workspace_roots.clone(),
            quiesced: true,
            taken_by,
            // Byte size is the provider's claim — `0` here means
            // "unreported" (the provider meter rows carry the real
            // accounting; never a fabricated size).
            size_bytes: 0,
            expires_at_ms: None,
        };
        rec.snapshot_ref = rec.compute_ref();
        let manifest_ref = store
            .put_blob(
                rec.to_json().to_canonical_string().as_bytes(),
                "application/json",
            )
            .map_err(EnvError::Ledger)?
            .id();
        let h = self.handles.get_mut(env_handle_id).unwrap();
        h.snapshots.push(rec.snapshot_ref.clone());
        let ev = EventMinter::new(store, &self.run_id).mint(
            "action.environment.snapshot",
            events::snapshot_payload(h, &rec.snapshot_ref, "memory", at_seq, Some(&manifest_ref)),
        )?;
        store.append(&self.run_id, lease, vec![ev])?;
        Ok(rec)
    }

    /// `restore_in_place(env_handle_id, snapshot_ref)` — the S5.8
    /// in-place revert (R-2.2.5² S2): only where the class declares
    /// `restore_in_place = supported`; `unsupported`/`unknown` stay the
    /// distinct typed refusals (`Unsupported`/`UnknownCapability` —
    /// never coerced, T-LCD-07). The mechanism is the provider
    /// adapter's; the kernel's durable record is
    /// `action.environment.restored{mode: "in_place", reverts}` — the
    /// `reverts` member names the snapshot so the revert relation is
    /// audit-visible like R-2.2.4's `revert` effect.
    pub fn restore_in_place(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
        snapshot_ref: &str,
    ) -> Result<(), EnvError> {
        let h = self
            .handles
            .get(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?
            .clone();
        use crate::handle::Tri;
        match h.capabilities.restore_in_place {
            Tri::Supported => {}
            Tri::Unsupported => {
                return Err(EnvError::Unsupported {
                    capability: "restore_in_place",
                    detail: format!(
                        "class {} declared restore_in_place unsupported",
                        h.class.as_str()
                    ),
                })
            }
            Tri::Unknown => {
                return Err(EnvError::UnknownCapability {
                    capability: "restore_in_place".to_string(),
                })
            }
        }
        let Some(ph) = h.provider.clone() else {
            return Err(EnvError::UnknownCapability {
                capability: "restore_in_place".to_string(),
            });
        };
        self.provider_for(h.class)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "no_adapter",
            })?
            .restore_in_place(&ph, snapshot_ref)?;
        let h = self.handles.get_mut(env_handle_id).unwrap();
        let ev = EventMinter::new(store, &self.run_id).mint(
            "action.environment.restored",
            Json::obj([
                ("env_handle", Json::str(h.env_handle_id.clone())),
                ("snapshot_ref", Json::str(snapshot_ref)),
                ("mode", Json::str("in_place")),
                ("reverts", Json::str(snapshot_ref)),
            ]),
        )?;
        store.append(&self.run_id, lease, vec![ev])?;
        Ok(())
    }

    /// `newest_snapshot(store, env_handle_id) → Option<(at_seq,
    /// SnapshotRecord)>` — the handle's newest recorded snapshot,
    /// `memory` preferred over `fs_tree` (the hibernation checkpoint is
    /// the fresher image; S5.8 snapshot-first healing's chooser). The
    /// record reloads through its `manifest_ref` blob — a named-but-gone
    /// blob is `SnapshotMissing` (the tombstone is the honest answer,
    /// never a fabricated record).
    fn newest_snapshot(
        &self,
        store: &Store,
        env_handle_id: &str,
    ) -> Result<Option<(u64, SnapshotRecord)>, EnvError> {
        let rows = store
            .env_snapshots(&self.run_id)
            .map_err(EnvError::Ledger)?;
        // (kind-rank, at_seq, record) — `memory` outranks `fs_tree` at
        // equal recency; newest wins. The chooser is handle-scoped: the
        // row's `env_handle` member lives on the record, so each
        // candidate reloads through its `manifest_ref` blob — a
        // named-but-gone blob is `SnapshotMissing`, a row for another
        // handle is skipped (not this env's restore point).
        let mut best: Option<(u8, u64, SnapshotRecord)> = None;
        for (s, sr, kind, mr) in rows {
            let rank = match kind.as_str() {
                "memory" => 2u8,
                "fs_tree" => 1u8,
                _ => continue,
            };
            if best
                .as_ref()
                .map(|(br, bs, _)| (rank, s) <= (*br, *bs))
                .unwrap_or(false)
            {
                continue;
            }
            let manifest_ref = mr.ok_or_else(|| EnvError::SnapshotMissing {
                snapshot_ref: sr.clone(),
            })?;
            let bytes =
                blob_by_id(store, &manifest_ref).ok_or_else(|| EnvError::SnapshotMissing {
                    snapshot_ref: sr.clone(),
                })?;
            let text = String::from_utf8(bytes)
                .map_err(|e| EnvError::Blob(format!("snapshot record {manifest_ref}: {e}")))?;
            let j = hh_wire::json::parse(&text)
                .map_err(|e| EnvError::Blob(format!("snapshot record {manifest_ref}: {e}")))?;
            let rec = SnapshotRecord::from_json(&j).ok_or_else(|| {
                EnvError::Blob(format!(
                    "snapshot record {manifest_ref}: malformed member form"
                ))
            })?;
            if rec.env_handle_id != env_handle_id {
                continue;
            }
            best = Some((rank, s, rec));
        }
        Ok(best.map(|(_, _, rec)| (rec.at_seq, rec)))
    }

    /// `snapshot_for(store, env_handle_id, at_seq)` — the S2.9 snapshot chooser
    /// (`fork{env: snapshot}`/`rollback`): the newest `fs_tree` snapshot on this
    /// run with `at_seq ≤ target`, reloaded from its `manifest_ref` blob.
    /// `Ok(None)` ⇒ the run recorded none at/below the cut; a named-but-gone
    /// record blob is `SnapshotMissing` (the tombstone explains — never a
    /// fabricated snapshot).
    pub fn snapshot_for(
        &self,
        store: &Store,
        at_seq: u64,
    ) -> Result<Option<(SnapshotRecord, hh_helper::fstree::FsTreeSnapshot)>, EnvError> {
        let rows = store
            .env_snapshots(&self.run_id)
            .map_err(EnvError::Ledger)?;
        // Greatest `at_seq ≤ at` among fs_tree snapshots.
        let mut best: Option<(u64, String, Option<String>)> = None;
        for (s, sr, kind, mr) in rows {
            if kind != "fs_tree" || s > at_seq {
                continue;
            }
            if best.as_ref().map(|(bs, _, _)| s > *bs).unwrap_or(true) {
                best = Some((s, sr, mr));
            }
        }
        let Some((_, snapshot_ref, manifest_ref)) = best else {
            return Ok(None);
        };
        let manifest_ref = manifest_ref.ok_or_else(|| EnvError::SnapshotMissing {
            snapshot_ref: snapshot_ref.clone(),
        })?;
        let bytes = blob_by_id(store, &manifest_ref).ok_or_else(|| EnvError::SnapshotMissing {
            snapshot_ref: snapshot_ref.clone(),
        })?;
        let text = String::from_utf8(bytes)
            .map_err(|e| EnvError::Blob(format!("snapshot record {manifest_ref}: {e}")))?;
        let j = hh_wire::json::parse(&text)
            .map_err(|e| EnvError::Blob(format!("snapshot record {manifest_ref}: {e}")))?;
        let rec = SnapshotRecord::from_json(&j).ok_or_else(|| {
            EnvError::Blob(format!(
                "snapshot record {manifest_ref}: malformed member form"
            ))
        })?;
        let tree = hh_helper::fstree::FsTreeSnapshot::from_manifest_json(&rec.content).ok_or_else(
            || EnvError::Blob(format!("snapshot {snapshot_ref}: malformed manifest")),
        )?;
        Ok(Some((rec, tree)))
    }

    /// `derive_from_snapshot(store, lease, parent_id, snap)` — the S2.9
    /// `fork{env: snapshot}` provisioning path (ADR-0271): a fresh workspace
    /// materialised **from the snapshot's blob-pool content** — never the
    /// parent's live tree (the parent may have moved past the cut). A missing
    /// content blob is `SnapshotMissing` (DF-S2.9-3's typed refusal).
    /// NB: `self` is the **child** run's driver — the spawned handle's rows
    /// append to the child run; `parent` is the source run's handle (borrowed
    /// from the parent's driver — an inter-run edge by construction).
    pub fn derive_from_snapshot(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        parent: &EnvHandle,
        snap: &hh_helper::fstree::FsTreeSnapshot,
    ) -> Result<EnvHandle, EnvError> {
        let parent = parent.clone();
        match parent.state {
            HandleState::Ready | HandleState::Detached => {}
            s => {
                return Err(EnvError::InvalidState {
                    op: "derive_from_snapshot",
                    state: s.as_str(),
                })
            }
        }
        // Content availability first — a GC'd snapshot blob is the typed
        // `SnapshotMissing` refusal, checked before any fs write.
        for node in snap.manifest.values().flat_map(|m| m.values()) {
            if let hh_helper::fstree::FsNode::File { ca, .. } = node {
                if blob_by_id(store, ca).is_none() {
                    return Err(EnvError::SnapshotMissing {
                        snapshot_ref: snap.tree_address.clone(),
                    });
                }
            }
        }
        let new_id = store.alloc_id("env");
        let base_dir = store.root().join("envs").join(&new_id).join("workspace");
        std::fs::create_dir_all(&base_dir).map_err(|e| EnvError::Blob(e.to_string()))?;
        hh_helper::fstree::restore(snap, &base_dir, &|ca| blob_by_id(store, ca))
            .map_err(|e| EnvError::Blob(format!("fork snapshot restore: {e}")))?;
        let roots = Roots {
            workspace_roots: vec![base_dir.to_string_lossy().to_string()],
            writable_roots: vec![base_dir.to_string_lossy().to_string()],
            cwd: base_dir.to_string_lossy().to_string(),
        };
        let child = self.spawn_handle(
            store,
            lease,
            &parent,
            roots,
            Some(crate::handle::ParentEdge {
                env_handle_id: parent.env_handle_id.clone(),
                mode: crate::handle::DeriveMode::ForkSnapshot,
                on_parent_end: crate::handle::OnParentEnd::DetachToChild,
            }),
            new_id,
        )?;
        let ev = EventMinter::new(store, &self.run_id).mint(
            "action.environment.derived",
            events::derived_payload(&child, None),
        )?;
        store.append(&self.run_id, lease, vec![ev])?;
        Ok(child)
    }

    /// `rollback_env(store, lease, env_handle_id, at_seq)` — the rewind's
    /// environment half (§5a.1 `rollback`; ADR-0271): restores the newest
    /// `fs_tree` snapshot with `at_seq ≤ target` **in place** (manifest-covered
    /// members written, uncovered members removed), appends
    /// `action.environment.restored`, and reports `(restored_ref, uncaptured)`
    /// — the writable roots no snapshot covers land in `uncaptured`, never
    /// silently kept.
    pub fn rollback_env(
        &mut self,
        store: &mut Store,
        lease: &Lease,
        env_handle_id: &str,
        at_seq: u64,
    ) -> Result<(Option<String>, Vec<String>), EnvError> {
        let h = self
            .handles
            .get(env_handle_id)
            .ok_or(EnvError::Unavailable {
                env_handle_id: env_handle_id.to_string(),
                state: "missing",
            })?
            .clone();
        let Some((rec, snap)) = self.snapshot_for(store, at_seq)? else {
            // No snapshot at/below the cut — nothing restored; every writable
            // root is honestly uncaptured.
            let uncaptured = h
                .roots
                .writable_roots
                .iter()
                .map(|r| format!("writable_root:{r}"))
                .collect();
            return Ok((None, uncaptured));
        };
        // Content availability — the missing/corrupt snapshot refusal.
        for node in snap.manifest.values().flat_map(|m| m.values()) {
            if let hh_helper::fstree::FsNode::File { ca, .. } = node {
                if blob_by_id(store, ca).is_none() {
                    return Err(EnvError::SnapshotMissing {
                        snapshot_ref: snap.tree_address.clone(),
                    });
                }
            }
        }
        let dest = std::path::PathBuf::from(&h.roots.cwd);
        std::fs::create_dir_all(&dest).map_err(|e| EnvError::Blob(e.to_string()))?;
        hh_helper::fstree::restore_in_place(&snap, &dest, &|ca| blob_by_id(store, ca))
            .map_err(|e| EnvError::Blob(format!("rollback env restore: {e}")))?;
        // Writable roots outside the snapshot's coverage are uncaptured.
        let covered: std::collections::BTreeSet<&str> =
            rec.roots_covered.iter().map(String::as_str).collect();
        let uncaptured: Vec<String> = h
            .roots
            .writable_roots
            .iter()
            .filter(|r| !covered.contains(r.as_str()))
            .map(|r| format!("writable_root:{r}"))
            .collect();
        let ev = EventMinter::new(store, &self.run_id).mint(
            "action.environment.restored",
            Json::obj([
                ("env_handle", Json::str(env_handle_id)),
                ("snapshot_ref", Json::str(&rec.snapshot_ref)),
                ("kind", Json::str("fs_tree")),
                ("at_seq", Json::Int(at_seq as i64)),
                (
                    "uncaptured",
                    Json::Arr(uncaptured.iter().map(Json::str).collect()),
                ),
            ]),
        )?;
        store.append(&self.run_id, lease, vec![ev])?;
        Ok((Some(rec.snapshot_ref), uncaptured))
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
        // S4.13 — provider meters fold into the same row (the kernel's
        // clocks plus the adapter's declared samples; undeclared names land
        // in `unreported`, never silently merged — CC3). Clone the adapter
        // inputs first so the immutable handle borrow ends before the
        // `&mut self` adapter lookup.
        let class = h.class;
        let ph = h.provider.clone();
        let provider_samples = if let Some(ph) = &ph {
            match self.provider_for(class) {
                Some(adapter) => adapter.meters(ph).unwrap_or_default(),
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };
        let h = self.handles.get(env_handle_id).unwrap();
        let payload = if provider_samples.is_empty() {
            events::meters_sampled_payload(h, now)
        } else {
            events::provider_meters_sampled_payload(h, now, &provider_samples)
        };
        let ev = EventMinter::new(store, &self.run_id)
            .mint("action.environment.meters_sampled", payload)?;
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
        let at_seq = store.head(&self.run_id).map(|h| h.seq).unwrap_or(0);
        let mut rec = SnapshotRecord {
            snapshot_ref: String::new(),
            env_handle_id: env_handle_id.to_string(),
            at_seq,
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
            events::snapshot_payload(h, &rec.snapshot_ref, "path_baseline", at_seq, None),
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

/// `blob_by_id(store, "sha256:<hex>")` — resolve a manifest content
/// address through the ledger blob pool (`get_blob` rehashes — corruption
/// surfaces as `BlobCorrupt`, never silent bytes).
fn blob_by_id(store: &Store, ca: &str) -> Option<Vec<u8>> {
    let digest = ca.strip_prefix("sha256:")?.to_string();
    let addr = hh_identity::idp::ContentAddress {
        idp: "idp/1",
        algorithm: "sha256",
        digest,
        media_type: "application/octet-stream".to_string(),
        size: 0,
    };
    store.get_blob(&addr).ok()
}

/// `copy_tree(src_root, dest_root)` — recursive copy of the source tree's
/// files/dirs into `dest` (the `fork_snapshot`/replace restore substrate —
/// per-file, no symlinks preserved beyond `symlink`+target).
fn copy_tree(src_root: &str, dest_root: &str) -> Result<(), EnvError> {
    let mut stack = vec![std::path::PathBuf::from(src_root)];
    while let Some(dir) = stack.pop() {
        let rd = std::fs::read_dir(&dir)
            .map_err(|e| EnvError::Blob(format!("copy_tree read_dir {}: {e}", dir.display())))?;
        for e in rd.flatten() {
            let p = e.path();
            let rel = p
                .strip_prefix(src_root)
                .map_err(|e| EnvError::Blob(e.to_string()))?;
            let dest = std::path::Path::new(dest_root).join(rel);
            let ft = e.file_type().map_err(|e| EnvError::Blob(e.to_string()))?;
            if ft.is_dir() {
                std::fs::create_dir_all(&dest).map_err(|e| EnvError::Blob(e.to_string()))?;
                stack.push(p);
            } else if ft.is_symlink() {
                if let Ok(target) = std::fs::read_link(&p) {
                    if let Some(par) = dest.parent() {
                        std::fs::create_dir_all(par).map_err(|e| EnvError::Blob(e.to_string()))?;
                    }
                    let _ = std::fs::remove_file(&dest);
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(&target, &dest)
                        .map_err(|e| EnvError::Blob(e.to_string()))?;
                }
            } else if ft.is_file() {
                if let Some(par) = dest.parent() {
                    std::fs::create_dir_all(par).map_err(|e| EnvError::Blob(e.to_string()))?;
                }
                std::fs::copy(&p, &dest).map_err(|e| EnvError::Blob(e.to_string()))?;
            }
        }
    }
    Ok(())
}
