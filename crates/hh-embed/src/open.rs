//! `open_session` — the resolve → validate → seal → provision → arm
//! chain (§7.4 §2.4). `new` runs the full pipeline over the submitted
//! definition; `resume`/`attach` bind to an existing run (the writer
//! lease is the fence; `attach` is read-only by construction — it takes
//! no lease and reaches no append path).

use crate::inject;
use crate::service::{
    ledger_err, EmbedService, HostAsk, HostCap, PendingAsk, SessionState, DEFAULT_MAX_SESSIONS,
    LEASE_TTL_MS,
};
use hh_assembly::resolve::{resolve, ResolveEnv};
use hh_assembly::validate::{validate_assembly, Subject};
use hh_containment::attach::{AttachMode, PolicySlot};
use hh_containment::policy::ResourceLimits;
use hh_control::driver::{Driver, DriverConfig};
use hh_control::output::SurfaceSpec;
use hh_control::policy::EnvelopePolicy;
use hh_control::react::ReactMinimal;
use hh_control::strategy::{ConcurrentInput, ControlContext, SteerMode, StrategyParams};
use hh_embed_schema::errors::EmbedError;
use hh_embed_schema::types::*;
use hh_env::driver::EnvDriver;
use hh_env::handle::{OnLoss, Roots};
use hh_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hh_identity::names::ResolveMode;
use hh_ledger::manifest::{AttendanceSource, AttendanceValue, RunKind, RunManifest};
use hh_wire::json::Json;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// The live control runtime a `resume` carries from the fenced
/// predecessor into the taking-over session — the driver leaf plus the
/// turn flags and pending-ask/idempotency tables (takeover continues the
/// same leaf under the new writer, so its pending asks stay answerable
/// and its idempotency keys still replay).
struct CarriedRuntime {
    driver: Driver<ReactMinimal>,
    env_json: Json,
    env_handle_id: Option<String>,
    host_caps: Vec<HostCap>,
    turn_active: bool,
    active_turn: String,
    pendings: BTreeMap<String, PendingAsk>,
    decided: BTreeMap<String, Json>,
    host_asks: BTreeMap<String, HostAsk>,
    idem: BTreeMap<String, Json>,
}

impl EmbedService {
    /// `open_session` — the three `OpenSpec` arms.
    pub(crate) fn open_session(&mut self, params: &Json) -> Result<Json, EmbedError> {
        inject::refuse_handle_keys(params, "open_session")?;
        inject::refuse_secrets(params)?;
        let p = OpenSessionParams::from_json(params)?;
        if let Some(hit) = self.open_idem.get(&p.idempotency_key) {
            return Ok(hit.clone());
        }
        let session = match &p.spec {
            OpenSpec::New {
                definition,
                overrides,
                environment,
                budget,
                profile_binding,
                participant,
                supplies,
                attendance,
                approval_mode,
            } => {
                inject::refuse_override_widening(overrides)?;
                self.open_new(
                    definition,
                    environment,
                    budget.as_ref(),
                    profile_binding.as_ref(),
                    participant.as_ref(),
                    supplies.as_ref(),
                    attendance,
                    approval_mode.as_deref(),
                )?
            }
            OpenSpec::Resume { run_id, mode, .. } => self.open_resume(run_id, mode)?,
            OpenSpec::Attach { run_id } => self.open_attach(run_id)?,
        };
        self.open_idem
            .insert(p.idempotency_key.clone(), session.clone());
        Ok(session)
    }

    /// The negotiated in-flight session bound (`0` = kernel default).
    fn session_cap(&self) -> usize {
        let n = self.client_caps.max_in_flight_sessions;
        if n > 0 {
            n as usize
        } else {
            DEFAULT_MAX_SESSIONS
        }
    }

    /// `open_session{kind:"new"}` — resolve → validate → seal →
    /// `open_run` → provision `local_host` → arm the driver.
    #[allow(clippy::too_many_arguments)]
    fn open_new(
        &mut self,
        definition: &DefinitionInput,
        environment: &EnvironmentInput,
        budget: Option<&BudgetInput>,
        profile_binding: Option<&Json>,
        participant: Option<&Json>,
        supplies: Option<&Supplies>,
        attendance: &AttendanceDeclaration,
        approval_mode: Option<&str>,
    ) -> Result<Json, EmbedError> {
        if self.sessions.len() >= self.session_cap() {
            return Err(EmbedError::Overloaded);
        }
        // Attendance gate — an unattended run must never wait on a human
        // (the approval mode or a requires-approval capability would).
        let cap_decl = parse_host_caps(supplies);
        if attendance.value == "unattended"
            && (approval_mode == Some("sync") || cap_decl.iter().any(|c| c.requires_approval))
        {
            return Err(EmbedError::UnattendedRequiresInput);
        }
        if let Some(p) = participant {
            let class = p.get("class").and_then(Json::as_str).unwrap_or("");
            if class != "native" {
                return Err(EmbedError::InvalidDefinition {
                    diagnostics: vec![format!(
                        "participant.class {class} is not `native` at Stage 1"
                    )],
                });
            }
        }

        // ── resolve → validate → seal ────────────────────────────────
        let sealed = self.resolve_definition(definition)?;
        let manifest_ref = sealed.definition_ref.version_id.clone();
        // The realized configuration pair — content-addressed through
        // `hh_assembly::configuration` (CC9): the manifest pins the
        // honest Stage-1 composition inputs (the scripted kernel model,
        // the declared profile binding, the realized environment, the
        // declared budget, seed `0` — `open_session` carries no seed at
        // Stage 1). The ledger's `open_run` gate requires pinned ids.
        let budget_input = match budget {
            Some(BudgetInput::Ref(r)) => r.clone(),
            Some(BudgetInput::Node(n)) => {
                hh_identity::address(n.to_canonical_string().as_bytes(), "application/json").id()
            }
            None => "unbudgeted".to_string(),
        };
        let cfg = hh_assembly::configuration(
            &sealed,
            &hh_assembly::CompositionInputs {
                model_ref: "hh-embed/kernel-scripted".to_string(),
                profile: profile_binding
                    .map(|p| p.to_canonical_string())
                    .unwrap_or_else(|| "profile:none".to_string()),
                environment_ref: "local_host".to_string(),
                budget: budget_input.clone(),
                seed: "0".to_string(),
            },
        );
        let configuration_id = cfg.configuration_id;
        let configuration_version_id = cfg.configuration_version_id;

        // ── manifest + open_run ──────────────────────────────────────
        let mut manifest = RunManifest::minimal(RunKind::Agent);
        manifest.configuration_id = Some(configuration_id.clone());
        manifest.configuration_version_id = Some(configuration_version_id.clone());
        manifest.harness_def_ref = Some(manifest_ref.clone());
        manifest.attendance = (
            AttendanceValue::parse(&attendance.value).unwrap_or(AttendanceValue::Async),
            AttendanceSource::parse(&attendance.source).unwrap_or(AttendanceSource::Declared),
        );
        manifest.budget = budget.map(|_| budget_input);
        let holder = self.holder.clone();
        let (run_id, lease) = self
            .store
            .open_run(manifest.clone(), &holder)
            .map_err(ledger_err)?;

        // ── environment: provision + attach local_host ───────────────
        let (env_handle_id, env_json) = self.provision_environment(&run_id, &lease, environment)?;

        // ── session + driver ─────────────────────────────────────────
        let session_id = self.alloc_session_id();
        self.mint(
            &run_id,
            &lease,
            "lifecycle.session.attached",
            Json::obj([
                ("session_id", Json::str(session_id.clone())),
                ("mode", Json::str("new")),
                ("attachment_id", Json::str(session_id.clone())),
            ]),
        )?;

        let surfaces = driver_surfaces(&cap_decl);
        let driver = self.arm_driver(&run_id, &lease, &surfaces, &manifest)?;
        let realized = realized_settings(self.workspace_root(), attendance, approval_mode);
        let head = self.store.head(&run_id).map_err(ledger_err)?;

        let sess = SessionState {
            run_id: run_id.clone(),
            attach: false,
            lease: Some(lease),
            manifest_ref: manifest_ref.clone(),
            realized: realized.clone(),
            driver: Some(driver),
            env_json,
            env_handle_id,
            host_caps: cap_decl.clone(),
            turn_active: false,
            active_turn: "turn-1".to_string(),
            finished: false,
            detached: None,
            pendings: BTreeMap::new(),
            decided: BTreeMap::new(),
            host_asks: BTreeMap::new(),
            idem: BTreeMap::new(),
            scan_seq: head.seq,
            next_invoke: None,
            next_completion: String::new(),
            next_response_ref: String::new(),
        };
        self.sessions.insert(session_id.clone(), sess);

        // Permission asks declared on the supplies — durable pending +
        // ephemeral requested + the upcall when the channel is served.
        for cap in cap_decl.iter().filter(|c| c.requires_approval) {
            let opts = if cap.options.is_empty() {
                vec!["allow_once".to_string(), "deny_once".to_string()]
            } else {
                cap.options.clone()
            };
            self.mint_permission_ask(
                &session_id,
                &format!("host capability {} requests approval", cap.capability_id),
                None,
                opts,
            )?;
        }

        Ok(session_json(
            &session_id,
            &run_id,
            &session_id,
            &manifest_ref,
            &configuration_id,
            &configuration_version_id,
            &realized,
            head.seq as i64,
        ))
    }

    /// `open_session{kind:"resume"}` — `continue` (WouldBlock while the
    /// writer lives) | `takeover` (fence the stale writer).
    fn open_resume(&mut self, run_id: &str, mode: &str) -> Result<Json, EmbedError> {
        if self.sessions.len() >= self.session_cap() {
            return Err(EmbedError::Overloaded);
        }
        // The run must exist and be unfinished.
        let events = self.store.events(run_id).map_err(ledger_err)?;
        if events.iter().any(|e| e.class == "lifecycle.run.finished") {
            return Err(EmbedError::Draining);
        }
        let holder = self.holder.clone();
        // The control leaf lives in the driver — a resume carries the
        // fenced predecessor's live runtime into the new session (the
        // same leaf continues under the new writer; no `turn-1` opener
        // is re-minted, and pending asks stay answerable). `takeover`
        // fences first (release → acquire bumps the generation);
        // `continue` surfaces WouldBlock while the writer is live.
        let mut carried: Option<CarriedRuntime> = None;
        let carry = |s: &mut SessionState, carried: &mut Option<CarriedRuntime>| {
            if carried.is_none() {
                *carried = s.driver.take().map(|d| CarriedRuntime {
                    driver: d,
                    env_json: std::mem::replace(&mut s.env_json, Json::Null),
                    env_handle_id: s.env_handle_id.take(),
                    host_caps: std::mem::take(&mut s.host_caps),
                    turn_active: s.turn_active,
                    active_turn: std::mem::take(&mut s.active_turn),
                    pendings: std::mem::take(&mut s.pendings),
                    decided: std::mem::take(&mut s.decided),
                    host_asks: std::mem::take(&mut s.host_asks),
                    idem: std::mem::take(&mut s.idem),
                });
            }
        };
        if mode == "takeover" {
            let stale: Vec<String> = self
                .sessions
                .iter()
                .filter(|(_, s)| {
                    s.run_id == run_id && !s.attach && s.lease.is_some() && s.detached.is_none()
                })
                .map(|(id, _)| id.clone())
                .collect();
            for sid in stale {
                if let Some(s) = self.sessions.get_mut(&sid) {
                    s.detached = Some("takeover".to_string());
                    carry(s, &mut carried);
                    if let Some(l) = s.lease.take() {
                        let _ = self.store.release(&l, "takeover");
                    }
                }
            }
        }
        let lease = self
            .store
            .acquire_writer(&holder, run_id, LEASE_TTL_MS)
            .map_err(ledger_err)?;
        // A successful acquire fences the previous writer — detach it and
        // carry its runtime (a `continue` past an expired lease resumes
        // the same live leaf).
        for s in self.sessions.values_mut() {
            if s.run_id == run_id && !s.attach {
                carry(s, &mut carried);
                if s.lease.as_ref().map(|l| l.generation) != Some(lease.generation)
                    && s.detached.is_none()
                {
                    s.detached = Some("fenced".to_string());
                }
            }
        }
        let session_id = self.alloc_session_id();
        self.mint(
            run_id,
            &lease,
            "lifecycle.session.attached",
            Json::obj([
                ("session_id", Json::str(session_id.clone())),
                ("mode", Json::str("resume")),
                ("attachment_id", Json::str(session_id.clone())),
            ]),
        )?;
        let manifest = self.store.manifest(run_id).map_err(ledger_err)?.clone();
        let rt = match carried {
            Some(rt) => rt,
            // The run carries driver rows but no live driver is in the
            // table — the leaf checkpoint isn't persisted at Stage 1, so
            // a cross-process resume can't be armed (DF-S1.25-2). Refuse
            // honestly rather than re-minting `turn-1` (DuplicateEventId).
            None => {
                return Err(EmbedError::Refused {
                    reason: "resume_checkpoint_unavailable".to_string(),
                })
            }
        };
        let realized = realized_settings(self.workspace_root(), &attendance_async(), None);
        let head = self.store.head(run_id).map_err(ledger_err)?;
        let sess = SessionState {
            run_id: run_id.to_string(),
            attach: false,
            lease: Some(lease),
            manifest_ref: manifest
                .harness_def_ref
                .clone()
                .unwrap_or_else(|| sess_manifest_ref(&manifest)),
            realized: realized.clone(),
            driver: Some(rt.driver),
            env_json: rt.env_json,
            env_handle_id: rt.env_handle_id,
            host_caps: rt.host_caps,
            turn_active: rt.turn_active,
            active_turn: rt.active_turn,
            finished: false,
            detached: None,
            pendings: rt.pendings,
            decided: rt.decided,
            host_asks: rt.host_asks,
            idem: rt.idem,
            scan_seq: head.seq,
            next_invoke: None,
            next_completion: String::new(),
            next_response_ref: String::new(),
        };
        self.sessions.insert(session_id.clone(), sess);
        Ok(session_json(
            &session_id,
            run_id,
            &session_id,
            &sess_manifest_ref(&manifest),
            &manifest.configuration_id.clone().unwrap_or_default(),
            &manifest
                .configuration_version_id
                .clone()
                .unwrap_or_default(),
            &realized,
            head.seq as i64,
        ))
    }

    /// `open_session{kind:"attach"}` — read-only always: no writer
    /// lease, no appends, no driver (I6).
    fn open_attach(&mut self, run_id: &str) -> Result<Json, EmbedError> {
        if self.sessions.len() >= self.session_cap() {
            return Err(EmbedError::Overloaded);
        }
        self.store.events(run_id).map_err(ledger_err)?;
        let manifest = self.store.manifest(run_id).map_err(ledger_err)?.clone();
        let head = self.store.head(run_id).map_err(ledger_err)?;
        let session_id = self.alloc_session_id();
        let realized = realized_settings(self.workspace_root(), &attendance_async(), None);
        let sess = SessionState {
            run_id: run_id.to_string(),
            attach: true,
            lease: None,
            manifest_ref: sess_manifest_ref(&manifest),
            realized: realized.clone(),
            driver: None,
            env_json: Json::Null,
            env_handle_id: None,
            host_caps: Vec::new(),
            turn_active: false,
            active_turn: "turn-1".to_string(),
            finished: false,
            detached: None,
            pendings: BTreeMap::new(),
            decided: BTreeMap::new(),
            host_asks: BTreeMap::new(),
            idem: BTreeMap::new(),
            scan_seq: head.seq,
            next_invoke: None,
            next_completion: String::new(),
            next_response_ref: String::new(),
        };
        self.sessions.insert(session_id.clone(), sess);
        Ok(session_json(
            &session_id,
            run_id,
            &format!("attach-{session_id}"),
            &sess_manifest_ref(&manifest),
            &manifest.configuration_id.clone().unwrap_or_default(),
            &manifest
                .configuration_version_id
                .clone()
                .unwrap_or_default(),
            &realized,
            head.seq as i64,
        ))
    }

    /// `close{reason}` — the writer's drain (interrupt → finished →
    /// release) or the attach session's detach; the result is the head
    /// coordinate at close (`Closed{final}`).
    pub(crate) fn close(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = CloseParams::from_json(params)?;
        let s = self.live_session(&p.session_id)?;
        let (run_id, attach, lease) = (s.run_id.clone(), s.attach, s.lease.clone());
        if !attach {
            let lease = lease.ok_or(EmbedError::Refused {
                reason: "session_is_read_only".to_string(),
            })?;
            // Mint the detach row while the run is live (a finished run
            // refuses appends — the lease release still lands).
            let finished = self.session(&p.session_id)?.finished
                || self
                    .store
                    .events(&run_id)
                    .map(|e| e.iter().any(|ev| ev.class == "lifecycle.run.finished"))
                    .unwrap_or(false);
            if !finished {
                self.mint(
                    &run_id,
                    &lease,
                    "lifecycle.session.detached",
                    Json::obj([
                        ("session_id", Json::str(p.session_id.clone())),
                        ("reason", Json::str(p.reason.clone())),
                    ]),
                )?;
                // Graceful close on an unfinished run — interrupt the
                // turn loop (cancelled-by-principal is the honest
                // Stage-1 close of live work).
                {
                    let s = self.session_mut(&p.session_id)?;
                    s.next_invoke = None;
                    s.next_completion = String::new();
                }
                if let Some(d) = self.session_mut(&p.session_id)?.driver.as_mut() {
                    d.submit(hh_control::vocab::Cue::HumanInput(
                        hh_control::vocab::HumanInput::Interrupt,
                    ));
                }
                let _ = self.drive(&p.session_id);
            }
            self.store.release(&lease, &p.reason).map_err(ledger_err)?;
        }
        let fin = self.summary_ref(&run_id);
        self.sessions.remove(&p.session_id);
        // The session's subscriptions are NOT dropped: the ticket is a
        // client-owned handle whose tail stays drainable past `close` —
        // the stream terminates on the ledger's own `Closed{RunFinished}`
        // frame (§7.4 §5), not on session teardown.
        Ok(Closed { final_summary: fin }.to_json())
    }

    // ── open helpers ────────────────────────────────────────────────────

    /// Resolve → validate → seal a `DefinitionInput` — the full
    /// `hh-assembly` pipeline (a `document` parses + resolves inside the
    /// embedded registry's snapshot; a `ref` has no publish path at the
    /// embed boundary at Stage 1 → `UnresolvedRef`, DF-S1.25-1).
    pub(crate) fn resolve_definition(
        &mut self,
        definition: &DefinitionInput,
    ) -> Result<hh_hir::document::SealedDefinition, EmbedError> {
        let doc_json = match definition {
            DefinitionInput::Document(j) => j.clone(),
            DefinitionInput::Ref(r) => {
                return Err(EmbedError::UnresolvedRef {
                    reference: r.clone(),
                })
            }
        };
        let bytes = doc_json.to_canonical_string().into_bytes();
        let loaded = hh_assembly::load::load(&bytes, None, &self.kernel_prov).map_err(|ds| {
            EmbedError::InvalidDefinition {
                diagnostics: ds.iter().map(diag_line).collect(),
            }
        })?;
        let load_errors: Vec<String> = loaded
            .diagnostics
            .iter()
            .filter(|d| matches!(d.severity, hh_assembly::diagnostics::Severity::Error))
            .map(diag_line)
            .collect();
        if !load_errors.is_empty() {
            return Err(EmbedError::InvalidDefinition {
                diagnostics: load_errors,
            });
        }
        let resolved_at = self.store.now_ms();
        let prov = self.kernel_prov.clone();
        let catalog = &self.catalog;
        let mut env = ResolveEnv {
            registry: &mut self.registry,
            catalog,
            snapshot_id: None,
            mode: ResolveMode::Execute,
            registrar: prov.clone(),
            resolved_at,
        };
        let sealed =
            resolve(&loaded.document, &mut env).map_err(|ds| EmbedError::InvalidDefinition {
                diagnostics: ds.iter().map(diag_line).collect(),
            })?;
        let report = validate_assembly(Subject::Sealed(&sealed), catalog, None, &prov);
        let errors: Vec<String> = report
            .diagnostics
            .iter()
            .filter(|d| matches!(d.severity, hh_assembly::diagnostics::Severity::Error))
            .map(diag_line)
            .collect();
        if !errors.is_empty() {
            return Err(EmbedError::InvalidDefinition {
                diagnostics: errors,
            });
        }
        // Land the sealed document's canonical bytes in the artifact table —
        // `get_artifact(version_id)` serves bytes that hash back to the id.
        self.sealed_defs.insert(
            sealed.definition_ref.version_id.clone(),
            sealed.canonical_bytes(),
        );
        Ok(sealed)
    }

    /// Provision + attach the session's environment. Stage 1 serves
    /// `local_host` (`EnvironmentInput::connection_info{class, roots?}`);
    /// anything else is a typed refusal.
    fn provision_environment(
        &mut self,
        run_id: &str,
        lease: &hh_ledger::store::Lease,
        environment: &EnvironmentInput,
    ) -> Result<(Option<String>, Json), EmbedError> {
        let info = match environment {
            EnvironmentInput::ConnectionInfo(c) => c.clone(),
            EnvironmentInput::Ref(r) => {
                return Err(EmbedError::UnresolvedRef {
                    reference: r.clone(),
                })
            }
        };
        let class = info
            .get("class")
            .and_then(Json::as_str)
            .unwrap_or("local_host");
        if class != "local_host" && class != "local" {
            return Err(EmbedError::EnvironmentUnavailable {
                reason: format!("environment class {class} is not served at Stage 1"),
            });
        }
        let ws = self.workspace_root().display().to_string();
        let roots_json = match info.get("workspace_roots") {
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>(),
            _ => vec![ws.clone()],
        };
        let record = EnvironmentRecord {
            class: EnvironmentClass::LocalHost,
            image: ImageRef::ContentAddress(hh_identity::address(
                format!("hh-embed/local_host:{ws}").as_bytes(),
                "application/vnd.hh.env",
            )),
            build_context: None,
            platform: std::env::consts::ARCH.to_string() + "-" + std::env::consts::OS,
            provisioning: ProvisioningRecipe::default(),
            containment_policy_ref: ("hh-embed/kernel_default".to_string(), "v1".to_string()),
            limits: ResourceLimits::default(),
            nondeterminism: vec![],
            unpinned: BTreeSet::new(),
            ext: BTreeMap::new(),
        };
        let roots = Roots {
            workspace_roots: roots_json.clone(),
            writable_roots: roots_json,
            cwd: ws.clone(),
        };
        let mut policy = hh_containment::policy::kernel_default(0);
        for r in &roots.writable_roots {
            policy
                .fs
                .write
                .allow
                .push(hh_containment::policy::WritableRoot {
                    root: r.clone(),
                    read_only_subpaths: vec![],
                    protected_metadata_names: vec![],
                });
        }
        policy.net.mode = hh_containment::policy::NetMode::None;
        policy.compute_ids();
        let driver = self
            .env_drivers
            .entry(run_id.to_string())
            .or_insert_with(|| EnvDriver::new(run_id));
        let handle = driver
            .provision(
                &mut self.store,
                lease,
                &record,
                roots,
                PolicySlot::Inline(Box::new(policy)),
                OnLoss::FailRun,
            )
            .map_err(env_err)?;
        // The Stage-1 backend is the EP2 reference model — the honest
        // in-process enforcement declaration (a real EP2 helper lands
        // with the containment surface at Stage 2). Fail-closed: the
        // report is stored and the applied row lands durable.
        let backend = hh_containment::backend::Ep2Model::reference();
        driver
            .attach(
                &mut self.store,
                lease,
                &handle.env_handle_id,
                Some(&backend),
                AttachMode::FailClosed,
                false,
                &[],
            )
            .map_err(env_err)?;
        let env_json = Json::obj([
            ("connection_info", info),
            ("health", Json::str("ready")),
            ("meters", Json::Arr(vec![])),
        ]);
        Ok((Some(handle.env_handle_id.clone()), env_json))
    }

    /// Arm the `ReactMinimal` driver over the session's run — the
    /// canonical control loop (`KernelSink` under the writer lease).
    fn arm_driver(
        &mut self,
        run_id: &str,
        lease: &hh_ledger::store::Lease,
        surfaces: &[SurfaceSpec],
        manifest: &RunManifest,
    ) -> Result<Driver<ReactMinimal>, EmbedError> {
        let ctx = ControlContext {
            process_ref: format!("hh-embed/{}", manifest.run_kind.as_str()),
            plan: vec![],
            boundary: hh_control::react::react_preset(),
            profile: Json::Null,
            account_ref: manifest
                .budget
                .clone()
                .unwrap_or_else(|| "acct:unbudgeted".to_string()),
            budget_ref: manifest.budget.clone().unwrap_or_else(|| "b-1".to_string()),
            envelope_ref: "env-1".to_string(),
            parameters: StrategyParams::default(),
            capabilities_available: surfaces.iter().map(|s| s.surface_id.clone()).collect(),
            steering: (SteerMode::Unsupported, ConcurrentInput::QueueOnly),
        };
        let policy = EnvelopePolicy::stage1_default(
            &manifest.budget.clone().unwrap_or_else(|| "b-1".to_string()),
        )
        .seal()
        .map_err(|e| EmbedError::Refused {
            reason: format!("envelope_policy: {e:?}"),
        })?;
        let mut sink = crate::runtime::KernelSink {
            store: &mut self.store,
            run_id: run_id.to_string(),
            lease: lease.clone(),
        };
        Driver::open_react(
            &ctx,
            policy,
            &mut sink,
            DriverConfig {
                surfaces: surfaces.to_vec(),
                ..DriverConfig::default()
            },
        )
        .map_err(|e| EmbedError::Refused {
            reason: format!("driver_open: {e:?}"),
        })
    }

    fn alloc_session_id(&mut self) -> String {
        self.alloc("sess")
    }

    pub(crate) fn workspace_root(&self) -> &std::path::Path {
        &self.workspace_root
    }
}

// ── module helpers ──────────────────────────────────────────────────────────

/// The attendance a `resume`/`attach` session reports — the declaration
/// travels on the source run's manifest; the boundary records `async`
/// declared here (I4: the realized projection is what open fixed).
pub(crate) fn attendance_async() -> AttendanceDeclaration {
    AttendanceDeclaration {
        value: "async".to_string(),
        source: "declared".to_string(),
    }
}

/// Parse `supplies.host_capabilities[]` records into the session's host
/// capability table. A record is `{capability_id, surface_id?,
/// requires_approval?, options?}` — the surface defaults to the
/// capability id (the model calls surfaces by name); `requires_approval`
/// defaults `false` (an undeclared ask is the dangerous default, not
/// the reverse).
pub(crate) fn parse_host_caps(supplies: Option<&Supplies>) -> Vec<HostCap> {
    let mut out = Vec::new();
    for c in supplies
        .map(|s| s.host_capabilities.as_slice())
        .unwrap_or(&[])
    {
        let capability_id = c
            .get("capability_id")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        if capability_id.is_empty() {
            continue;
        }
        let surface_id = c
            .get("surface_id")
            .and_then(Json::as_str)
            .unwrap_or(capability_id.as_str())
            .to_string();
        let requires_approval = matches!(c.get("requires_approval"), Some(Json::Bool(true)));
        let options = match c.get("options") {
            Some(Json::Arr(a)) => a
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            _ => Vec::new(),
        };
        out.push(HostCap {
            capability_id,
            surface_id,
            requires_approval,
            options,
        });
    }
    out
}

/// The `hh.submit` completion surface — the one kernel-dispatched
/// capability the scripted model port calls.
pub(crate) fn submit_surface_spec() -> SurfaceSpec {
    SurfaceSpec {
        surface_id: crate::runtime::SUBMIT_SURFACE.to_string(),
        semantic_id: format!("{}/1", crate::runtime::SUBMIT_SURFACE),
        params: std::collections::BTreeMap::new(),
    }
}

/// The driver's surface table — the completion surface plus every
/// declared host-executor surface (the model may call only what the
/// definition supplied; the gate refuses anything else loudly).
pub(crate) fn driver_surfaces(caps: &[HostCap]) -> Vec<SurfaceSpec> {
    let mut v = vec![submit_surface_spec()];
    for c in caps {
        v.push(SurfaceSpec {
            surface_id: c.surface_id.clone(),
            semantic_id: c.capability_id.clone(),
            params: std::collections::BTreeMap::new(),
        });
    }
    v
}

/// The `open_session`/`fork` result record (the contract's `Session`) —
/// the parameter list is the record's own field list (a constructor,
/// not a call graph).
#[allow(clippy::too_many_arguments)]
pub(crate) fn session_json(
    session_id: &str,
    run_id: &str,
    attachment_id: &str,
    manifest_ref: &str,
    configuration_id: &str,
    configuration_version_id: &str,
    realized: &RealizedSettings,
    cursor_seq: i64,
) -> Json {
    Session {
        session_id: session_id.to_string(),
        run_id: run_id.to_string(),
        attachment_id: attachment_id.to_string(),
        manifest_ref: manifest_ref.to_string(),
        configuration_id: configuration_id.to_string(),
        configuration_version_id: configuration_version_id.to_string(),
        realized: realized.clone(),
        cursor_seq,
    }
    .to_json()
}

/// The `manifest_ref` a resume/attach session reports — the sealed
/// definition's version id when the manifest binds one (I4: identical
/// bytes to what `new` recorded).
pub(crate) fn sess_manifest_ref(manifest: &RunManifest) -> String {
    manifest
        .harness_def_ref
        .clone()
        .or_else(|| manifest.configuration_version_id.clone())
        .unwrap_or_default()
}

/// The `RealizedSettings` the boundary realizes at Stage 1 — the honest
/// minimal projection (the model role table is the scripted kernel
/// port's; the containment record is the kernel default the environment
/// provisioned under).
pub(crate) fn realized_settings(
    workspace_root: &std::path::Path,
    attendance: &AttendanceDeclaration,
    approval_mode: Option<&str>,
) -> RealizedSettings {
    RealizedSettings {
        model_role_table_realized: Json::obj([(
            "roles",
            Json::obj([("primary", Json::str("hh-embed/kernel-scripted"))]),
        )]),
        cwd: workspace_root.display().to_string(),
        containment_effective: Json::obj([("profile", Json::str("kernel_default"))]),
        policy_mode: approval_mode.unwrap_or("observe_only").to_string(),
        profile_bindings: Json::Arr(vec![]),
        protocol_bindings: vec![],
        secrets_declared: vec![],
        attendance: attendance.clone(),
    }
}

/// Fold an `EnvError` into the contract's surface — provisioning and
/// attach failures are `EnvironmentUnavailable` (the environment's
/// words, carried in `reason`; never a stringy `Internal`).
pub(crate) fn env_err(e: hh_env::errors::EnvError) -> EmbedError {
    EmbedError::EnvironmentUnavailable {
        reason: format!("{e:?}"),
    }
}

/// One diagnostic line for `InvalidDefinition.diagnostics` — the
/// canonical `C-…` code, the JSON-pointer path, and the detail text
/// (never the whole record: clients render strings).
pub(crate) fn diag_line(d: &hh_assembly::diagnostics::AssemblyDiagnostic) -> String {
    format!(
        "{:?} {} {} — {}",
        d.code,
        d.path,
        d.subject,
        d.detail.content.as_deref().unwrap_or("")
    )
}
