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
use hh_control::output::{ParamKind, ParamSpec, SurfaceSpec};
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
    budget_ceiling: BTreeMap<String, i64>,
    /// The woken-delivery dedup set — an in-process takeover keeps it
    /// (no re-delivery); a durable resume starts empty (at-least-once
    /// into the new writer's inbox is the intended crash semantics).
    delivered_wokens: BTreeSet<String>,
    /// The resume-by-leaf arm record — carried across an in-process
    /// takeover, re-read from `leaf.arm` on a durable resume.
    leaf_arm: LeafArm,
}

/// The `leaf.arm` record (S2.3) — the arm-time driver config a durable
/// resume re-reads (`leaf.checkpoint` restores the leaf *state*; `leaf.arm`
/// restores the leaf *config*: surfaces + the arm-time budget maps the
/// checkpoint doesn't carry).
#[derive(Debug, Clone, Default)]
pub(crate) struct LeafArm {
    pub surfaces: Vec<SurfaceSpec>,
    pub budget_ceiling: BTreeMap<String, i64>,
    pub remaining: BTreeMap<String, i64>,
}

impl LeafArm {
    fn to_json(&self) -> Json {
        Json::obj([
            (
                "surfaces",
                Json::Arr(self.surfaces.iter().map(surface_json).collect()),
            ),
            (
                "budget_ceiling",
                Json::Obj(
                    self.budget_ceiling
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::Int(*v)))
                        .collect(),
                ),
            ),
            (
                "remaining",
                Json::Obj(
                    self.remaining
                        .iter()
                        .map(|(k, v)| (k.clone(), Json::Int(*v)))
                        .collect(),
                ),
            ),
        ])
    }

    fn from_json(j: &Json) -> Option<LeafArm> {
        let int_map = |key: &str| -> BTreeMap<String, i64> {
            match j.get(key) {
                Some(Json::Obj(m)) => m
                    .iter()
                    .filter_map(|(k, v)| v.as_int().map(|n| (k.clone(), n)))
                    .collect(),
                _ => BTreeMap::new(),
            }
        };
        let surfaces = match j.get("surfaces") {
            Some(Json::Arr(a)) => a.iter().filter_map(surface_from_json).collect(),
            _ => Vec::new(),
        };
        Some(LeafArm {
            surfaces,
            budget_ceiling: int_map("budget_ceiling"),
            remaining: int_map("remaining"),
        })
    }
}

fn surface_json(s: &SurfaceSpec) -> Json {
    Json::obj([
        ("surface_id", Json::str(s.surface_id.clone())),
        ("semantic_id", Json::str(s.semantic_id.clone())),
        (
            "params",
            Json::Obj(
                s.params
                    .iter()
                    .map(|(k, v)| (k.clone(), param_json(v)))
                    .collect(),
            ),
        ),
    ])
}

fn param_json(p: &ParamSpec) -> Json {
    Json::obj([
        ("required", Json::Bool(p.required)),
        ("kind", Json::str(param_kind_str(p.kind).to_string())),
        ("enum_values", Json::Arr(p.enum_values.clone())),
        ("domain", Json::Arr(p.domain.clone())),
    ])
}

fn param_kind_str(k: ParamKind) -> &'static str {
    match k {
        ParamKind::Any => "any",
        ParamKind::Str => "string",
        ParamKind::Int => "int",
        ParamKind::Bool => "bool",
        ParamKind::Arr => "arr",
        ParamKind::Obj => "obj",
    }
}

fn param_kind_from(s: &str) -> ParamKind {
    match s {
        "string" => ParamKind::Str,
        "int" => ParamKind::Int,
        "bool" => ParamKind::Bool,
        "arr" => ParamKind::Arr,
        "obj" => ParamKind::Obj,
        _ => ParamKind::Any,
    }
}

fn surface_from_json(j: &Json) -> Option<SurfaceSpec> {
    let surface_id = j.get("surface_id").and_then(Json::as_str)?.to_string();
    let semantic_id = j
        .get("semantic_id")
        .and_then(Json::as_str)
        .unwrap_or_default()
        .to_string();
    let mut params = BTreeMap::new();
    if let Some(Json::Obj(m)) = j.get("params") {
        for (k, v) in m {
            params.insert(
                k.clone(),
                ParamSpec {
                    required: matches!(v.get("required"), Some(Json::Bool(true))),
                    kind: param_kind_from(v.get("kind").and_then(Json::as_str).unwrap_or("any")),
                    enum_values: match v.get("enum_values") {
                        Some(Json::Arr(a)) => a.clone(),
                        _ => Vec::new(),
                    },
                    domain: match v.get("domain") {
                        Some(Json::Arr(a)) => a.clone(),
                        _ => Vec::new(),
                    },
                },
            );
        }
    }
    Some(SurfaceSpec {
        surface_id,
        semantic_id,
        params,
    })
}

impl EmbedService {
    /// `open_session` — the three `OpenSpec` arms.
    pub(crate) fn open_session(&mut self, params: &Json) -> Result<Json, EmbedError> {
        inject::refuse_handle_keys(params, "open_session")?;
        inject::refuse_secrets(params)?;
        let p = OpenSessionParams::from_json(params)?;
        // A session id is a live handle — idempotency dedupes the
        // *open*, not the handle's lifetime. When the recorded session
        // is gone the verbatim replay would hand back a dead id
        // (`UnknownSession` on the next call):
        //  - `new`: the run is the idempotent product — mint a fresh
        //    attach handle on the recorded `run_id` so the retried
        //    invocation sees the same run (the caller finds
        //    `lifecycle.run.finished` in the durable prefix and skips
        //    `submit`).
        //  - `attach`/`resume`: fall through — the open is itself
        //    idempotent, so mint a fresh session for the same spec.
        enum Replay {
            None,
            Verbatim(Json),
            AttachOf(String),
        }
        let replay = match self.open_idem.get(&p.idempotency_key) {
            Some(hit) => {
                let sid_live = hit
                    .get("session_id")
                    .and_then(Json::as_str)
                    .map(|sid| self.sessions.contains_key(sid))
                    .unwrap_or(false);
                if sid_live {
                    Replay::Verbatim(hit.clone())
                } else if matches!(&p.spec, OpenSpec::New { .. }) {
                    Replay::AttachOf(
                        hit.get("run_id")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_string(),
                    )
                } else {
                    Replay::None
                }
            }
            None => Replay::None,
        };
        match replay {
            Replay::Verbatim(hit) => return Ok(hit),
            Replay::AttachOf(run_id) => return self.open_attach(&run_id),
            Replay::None => {}
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
            } => self.open_new(
                definition,
                overrides,
                environment,
                budget.as_ref(),
                profile_binding.as_ref(),
                participant.as_ref(),
                supplies.as_ref(),
                attendance,
                approval_mode.as_deref(),
                p.invocation.as_ref(),
            )?,
            OpenSpec::Resume {
                run_id,
                mode,
                definition,
                ..
            } => self.open_resume(run_id, mode, definition.as_ref(), p.invocation.as_ref())?,
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
    /// override desugar + I-1 → `open_run` → provision `local_host` →
    /// arm the driver.
    #[allow(clippy::too_many_arguments)]
    fn open_new(
        &mut self,
        definition: &DefinitionInput,
        overrides: &[Override],
        environment: &EnvironmentInput,
        budget: Option<&BudgetInput>,
        profile_binding: Option<&Json>,
        participant: Option<&Json>,
        supplies: Option<&Supplies>,
        attendance: &AttendanceDeclaration,
        approval_mode: Option<&str>,
        invocation: Option<&InvocationRecord>,
    ) -> Result<Json, EmbedError> {
        // `max_in_flight_sessions` bounds *live* sessions — a fenced or
        // detached session is residue, not in-flight (the negotiated
        // cap would otherwise deadlock the parked-run workflow: a
        // detached-CLI writer plus the next invocation's session).
        let live = self
            .sessions
            .values()
            .filter(|s| s.detached.is_none())
            .count();
        if live >= self.session_cap() {
            return Err(EmbedError::Overloaded);
        }
        // Attendance gate (M-2/S-1, ADR-0168 D2) — an unattended run must
        // never wait on a human, and prompting tools are never *excluded*
        // (that would change the compiled surface and the
        // `configuration_id`): a requires-approval capability under
        // `unattended` resolves by Π-12 below — `decided{decider: policy,
        // reason: unattended}` — never a refusal. The refusal case is a
        // declared mode that *demands* a human (`manual`/`tiered`/
        // `auto_review`) under unattended attendance — that combination
        // can only hang, so `UnattendedRequiresInput` fires pre-ledger.
        let cap_decl = parse_host_caps(supplies);
        let human_mode = matches!(
            approval_mode,
            Some("manual") | Some("tiered") | Some("auto_review") | Some("sync")
        );
        if attendance.value == "unattended" && human_mode {
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

        // ── override desugar + I-1 (before `open_run` — a refusal means
        // no run ever existed) ───────────────────────────────────────
        let overrides_layer_id = self.materialise_overrides(&sealed, overrides, attendance)?;
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
        manifest.overrides_layer_id = overrides_layer_id.clone();
        let holder = self.holder.clone();
        let (run_id, lease) = self
            .store
            .open_run(manifest.clone(), &holder)
            .map_err(ledger_err)?;

        // `lifecycle.surface.invoked` — the durable invocation record
        // minted when the invocation opens a run (§7.1; the record's
        // `overrides_layer_id` is the boundary-computed layer identity,
        // filled here so a surface caller may omit it).
        if let Some(inv) = invocation {
            let mut payload = inv.to_json();
            if let (Some(layer), Json::Obj(m)) = (&overrides_layer_id, &mut payload) {
                m.insert("overrides_layer_id".into(), Json::str(layer.clone()));
            }
            self.mint(&run_id, &lease, "lifecycle.surface.invoked", payload)?;
        }

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
        let driver = self.arm_driver(&run_id, &lease, &surfaces, &manifest, budget)?;
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
            delivered_wokens: BTreeSet::new(),
            host_asks: BTreeMap::new(),
            idem: BTreeMap::new(),
            budget_ceiling: budget_dimensions(budget).0,
            leaf_arm: LeafArm {
                surfaces: surfaces.clone(),
                budget_ceiling: budget_dimensions(budget).0,
                remaining: budget_dimensions(budget).1,
            },
            scan_seq: head.seq,
            next_invoke: None,
            next_completion: String::new(),
            next_response_ref: String::new(),
        };
        self.sessions.insert(session_id.clone(), sess);
        // Persist the resume-by-leaf pair (S2.3) — a resume right
        // after open restores the leaf exactly where `open` left it.
        self.persist_leaf(&session_id);

        // Permission asks declared on the supplies — durable pending +
        // ephemeral requested + the upcall when the channel is served.
        // Under `unattended` attendance every ask resolves by Π-12:
        // `pending` then `decided{decider: policy, reason: unattended}`
        // (M-2 — the asks are ledgered, never silently auto-rejected and
        // never excluded from the surface).
        let unattended = attendance.value == "unattended";
        for cap in cap_decl.iter().filter(|c| c.requires_approval) {
            let proposal = format!("host capability {} requests approval", cap.capability_id);
            if unattended {
                let permission_id = self.alloc("perm");
                let (run_id, lease) = {
                    let s = self.session(&session_id)?;
                    (
                        s.run_id.clone(),
                        s.lease.clone().ok_or(EmbedError::Refused {
                            reason: "session_is_read_only".to_string(),
                        })?,
                    )
                };
                let now = self.store.now_ms();
                self.mint(
                    &run_id,
                    &lease,
                    "security.permission.pending",
                    Json::obj([
                        ("permission_id", Json::str(permission_id.clone())),
                        ("effect_ids", Json::Arr(vec![])),
                        ("requested_at", Json::Int(now as i64)),
                        ("mode", Json::str("sync")),
                    ]),
                )?;
                self.mint(
                    &run_id,
                    &lease,
                    "security.permission.decided",
                    Json::obj([
                        ("permission_id", Json::str(permission_id)),
                        ("proposal", Json::str(proposal)),
                        ("decision", Json::str("deny")),
                        ("decider", Json::str("policy")),
                        ("reason", Json::str("unattended")),
                    ]),
                )?;
            } else {
                let opts = if cap.options.is_empty() {
                    vec!["allow_once".to_string(), "deny_once".to_string()]
                } else {
                    cap.options.clone()
                };
                // AC-R-2.11.1-12 — the ask carries the fields the
                // surface renders: the untruncated proposal (`text`)
                // plus the `Explanation` members the declaration
                // supports (the firing declaration, the identifiers the
                // ask concerns, and what would auto-approve).
                // `model_justification` is present only when a delegate
                // authored the reason — capability-declared asks carry
                // none.
                let rendering = Json::obj([
                    ("text", Json::str(proposal.clone())),
                    (
                        "explanation",
                        Json::obj([
                            ("rule_id", Json::str("capability.requires_approval")),
                            (
                                "risk_factors",
                                Json::Arr(vec![
                                    Json::str(format!("capability_id:{}", cap.capability_id)),
                                    Json::str(format!("surface_id:{}", cap.surface_id)),
                                ]),
                            ),
                            (
                                "what_would_auto_approve",
                                Json::obj([("requires_approval", Json::Bool(false))]),
                            ),
                        ]),
                    ),
                ]);
                self.mint_permission_ask(&session_id, &proposal, None, opts, rendering, Some(cap))?;
            }
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
    /// writer lives) | `takeover` (fence the stale writer). A supplied
    /// `definition` is the resume-time re-presentation (AC-R-2.2.3-11;
    /// §3.3.4 `verify_resume`): identical ⇒ continue; add-only ⇒
    /// `lifecycle.definition.changed` + continue; otherwise
    /// `DefinitionChanged` — checked *before* any takeover, so a refusal
    /// leaves the ledger untouched.
    fn open_resume(
        &mut self,
        run_id: &str,
        mode: &str,
        definition: Option<&DefinitionInput>,
        invocation: Option<&InvocationRecord>,
    ) -> Result<Json, EmbedError> {
        // Live sessions bound the cap; the target run's own writers are
        // excluded — `takeover` fences them below, `continue` answers
        // `WouldBlock` at the lease.
        let live = self
            .sessions
            .values()
            .filter(|s| s.detached.is_none() && (s.attach || s.run_id != run_id))
            .count();
        if live >= self.session_cap() {
            return Err(EmbedError::Overloaded);
        }
        // The run must exist and be unfinished.
        let events = self.store.events(run_id).map_err(ledger_err)?;
        if events.iter().any(|e| e.class == "lifecycle.run.finished") {
            return Err(EmbedError::Draining);
        }
        // ── verify_resume (AC-R-2.2.3-11; ADR-0025) ──────────────────
        // A re-presented definition is verified against the persisted
        // sealed definition *before* the takeover path touches anything:
        // identical ⇒ nothing minted; add-only ⇒ the `Compatible` diff is
        // recorded as `lifecycle.definition.changed` once the lease lands;
        // incompatible ⇒ `DefinitionChanged` with the full reason set.
        let mut definition_changed: Option<(String, String, String)> = None;
        let mut resume_prov: Option<hh_provenance::ProvenanceRecord> = None;
        if let Some(d) = definition {
            let current = self.resolve_definition(d)?;
            let manifest = self.store.manifest(run_id).map_err(ledger_err)?.clone();
            let Some(persisted_ref) = manifest.harness_def_ref.clone() else {
                return Err(EmbedError::DefinitionChanged {
                    reasons: vec!["no persisted definition ref to verify against".to_string()],
                });
            };
            if current.definition_ref.version_id != persisted_ref {
                let persisted = self.persisted_definition(&persisted_ref)?;
                let view = hh_assembly::LedgerView {
                    events: self.store.events(run_id).map_err(ledger_err)?,
                    // Mid-run budget decreases need the accounting gate —
                    // no admission path exists at this seam, so the gate
                    // is `DenyAll` (a decrease is `Incompatible`).
                    budget_gate: &hh_assembly::DenyAll,
                };
                // The re-presented edit is the operator's — human-origin
                // (`verify_resume`'s diff gates read the origin; the
                // recorded `definition.changed` row carries it too).
                let prov = hh_provenance::ProvenanceRecord::minted(
                    hh_provenance::Origin::human(
                        "operator:resume",
                        hh_provenance::HumanRole::Author,
                    ),
                    hh_provenance::PersistenceScope::Definition,
                    self.store.now_ms(),
                );
                match hh_assembly::verify_resume(
                    &persisted,
                    &current,
                    &view,
                    prov.clone(),
                    hh_hir::diff::DiffDerivation::default(),
                ) {
                    Ok(hh_assembly::ResumeVerdict::Compatible { diff, diff_ref }) => {
                        // The `diff_ref` the row names must resolve —
                        // deposit the canonical diff into the blob pool
                        // (CC3; `diff_ref` is a blob-domain address).
                        let dj = hh_hir::wire::diff_to_json(&diff);
                        self.store
                            .put_blob(
                                dj.to_canonical_string().as_bytes(),
                                "application/x-hir-diff",
                            )
                            .map_err(ledger_err)?;
                        definition_changed = Some((
                            current.definition_ref.semantic_id.clone(),
                            current.definition_ref.version_id.clone(),
                            diff_ref,
                        ));
                        resume_prov = Some(prov);
                    }
                    Ok(hh_assembly::ResumeVerdict::Incompatible { reasons }) => {
                        return Err(EmbedError::DefinitionChanged {
                            reasons: reasons
                                .iter()
                                .map(|r| format!("{}: {} ({})", r.rule.as_str(), r.path, r.detail))
                                .collect(),
                        });
                    }
                    Err(diags) => {
                        return Err(EmbedError::InvalidDefinition {
                            diagnostics: diags.iter().map(diag_line).collect(),
                        })
                    }
                }
            }
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
                    budget_ceiling: std::mem::take(&mut s.budget_ceiling),
                    delivered_wokens: std::mem::take(&mut s.delivered_wokens),
                    leaf_arm: std::mem::take(&mut s.leaf_arm),
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
        // Durable resume (S2.3; DF-S1.25-2) — no live driver in the table:
        // `restore` performs the takeover itself (generation-fences any
        // stale writer, writes the recovery rows + the audited
        // `lifecycle.run.resumed{recovery_decision}`) and returns the
        // session's writer lease. Refuse *before* the takeover when no
        // persisted leaf checkpoint exists — a resume without one can't be
        // armed, and the refusal leaves the ledger untouched.
        let needs_durable_resume = carried.is_none();
        if needs_durable_resume {
            let ckpt = self
                .store
                .root()
                .join("runs")
                .join(run_id)
                .join("leaf.checkpoint");
            if !ckpt.exists() {
                return Err(EmbedError::Refused {
                    reason: "resume_checkpoint_unavailable".to_string(),
                });
            }
        }
        let (lease, restore_report) = if needs_durable_resume {
            let report = self
                .store
                .restore_caused(run_id, &holder, LEASE_TTL_MS, "operator")
                .map_err(ledger_err)?;
            (report.lease.clone(), Some(report))
        } else {
            (
                self.store
                    .acquire_writer(&holder, run_id, LEASE_TTL_MS)
                    .map_err(ledger_err)?,
                None,
            )
        };
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
        // The accepted resume change's durable record (AC-R-2.2.3-11) —
        // `lifecycle.definition.changed{definition_ref, diff_ref, reasons[]}`
        // under the operator's human origin (ADR-0066's provenance rule).
        if let Some((semantic_id, version_id, diff_ref)) = definition_changed {
            // The class is kernel-origin-gated — the *row's* provenance
            // is the kernel's; the human origin lives on the diff record
            // (`diff.provenance`) the `diff_ref` resolves to.
            let prov = self.kernel_prov.clone();
            let _ = resume_prov;
            hh_assembly::emit(
                &mut self.store,
                run_id,
                &lease,
                &prov,
                vec![hh_assembly::events::definition_changed(
                    &semantic_id,
                    &version_id,
                    &diff_ref,
                    &[],
                )],
            )
            .map_err(ledger_err)?;
        }
        let manifest = self.store.manifest(run_id).map_err(ledger_err)?.clone();
        // `lifecycle.surface.invoked` — a resume writes to the run, so a
        // surface invocation mints its durable record here too.
        if let Some(inv) = invocation {
            self.mint(run_id, &lease, "lifecycle.surface.invoked", inv.to_json())?;
        }
        let rt = match carried {
            Some(rt) => rt,
            // The run carries driver rows but no live driver is in the
            // table (service restart / cross-process resume) — resume-by-leaf:
            // `restore` already landed; re-arm the leaf from the persisted
            // checkpoint and observe the durable tail (DF-S1.25-2 → S2.3).
            None => {
                let ckpt = self
                    .store
                    .root()
                    .join("runs")
                    .join(run_id)
                    .join("leaf.checkpoint");
                let bytes = std::fs::read(&ckpt).map_err(|_| EmbedError::Refused {
                    reason: "resume_checkpoint_unavailable".to_string(),
                })?;
                let (driver, leaf_arm) =
                    self.arm_resume_driver(run_id, &lease, &bytes, &manifest)?;
                // The owed-permission table rebuilds from the durable
                // `security.permission.pending` rows the restore surfaced —
                // `respond` stays answerable across the restart (§5g.7 §5).
                let mut pendings = BTreeMap::new();
                if let Some(rep) = &restore_report {
                    for pid in &rep.pending_permissions {
                        let row = self
                            .store
                            .events(run_id)
                            .map_err(ledger_err)?
                            .iter()
                            .rev()
                            .find(|e| {
                                e.class == "security.permission.pending"
                                    && e.payload.get("permission_id").and_then(Json::as_str)
                                        == Some(pid.as_str())
                            });
                        if let Some(e) = row {
                            let req = e.payload.get("request").cloned().unwrap_or(Json::Null);
                            pendings.insert(
                                pid.clone(),
                                PendingAsk {
                                    options: vec![
                                        "allow_once".to_string(),
                                        "allow_lease".to_string(),
                                        "deny".to_string(),
                                        "more_info".to_string(),
                                    ],
                                    proposal: req
                                        .get("reason")
                                        .and_then(Json::as_str)
                                        .unwrap_or("permission request")
                                        .to_string(),
                                    effect_id: e
                                        .payload
                                        .get("effect_id")
                                        .and_then(Json::as_str)
                                        .map(str::to_string),
                                    requested_at: e
                                        .payload
                                        .get("requested_at")
                                        .and_then(Json::as_int)
                                        .map(|n| n.max(0) as u64)
                                        .unwrap_or(0),
                                    capability_ref: req.get("capability_ref").and_then(|c| {
                                        let s = c.get("semantic_id")?.as_str()?;
                                        let v = c.get("version_id")?.as_str()?;
                                        Some((s.to_string(), v.to_string()))
                                    }),
                                    args_canonical_hash: req
                                        .get("args_canonical_hash")
                                        .and_then(Json::as_str)
                                        .map(str::to_string),
                                    subject_ref: req
                                        .get("subject_ref")
                                        .and_then(Json::as_str)
                                        .map(str::to_string),
                                },
                            );
                        }
                    }
                }
                // `verify_environment` on resume (§5a.3 C0 — every handle in
                // the checkpoint view is re-verified; the verdict is the
                // `action.environment.verified` row, never the stale handle).
                if let Some(drv) = self.env_drivers.get_mut(run_id) {
                    let ids: Vec<String> = drv.handle_ids();
                    for h in ids {
                        let _ = drv.verify_environment_verdict(&mut self.store, &lease, &h);
                    }
                }
                // The active turn — the durable prefix's last un-finished
                // `turn.started` (the boundary's `active_turn` check reads
                // the fold, never a guess).
                let (mut active, mut finished_turns) = (String::new(), BTreeSet::new());
                for e in self.store.events(run_id).map_err(ledger_err)? {
                    match e.class.as_str() {
                        "lifecycle.turn.started" => {
                            active = e
                                .payload
                                .get("turn_id")
                                .and_then(Json::as_str)
                                .unwrap_or_default()
                                .to_string();
                        }
                        "lifecycle.turn.finished" => {
                            if let Some(t) = e
                                .payload
                                .get("turn_id")
                                .and_then(Json::as_str)
                                .map(str::to_string)
                            {
                                finished_turns.insert(t);
                            }
                        }
                        _ => {}
                    }
                }
                CarriedRuntime {
                    driver,
                    env_json: Json::Null,
                    env_handle_id: None,
                    host_caps: Vec::new(),
                    turn_active: !active.is_empty() && !finished_turns.contains(&active),
                    active_turn: if active.is_empty() {
                        "turn-1".to_string()
                    } else {
                        active
                    },
                    pendings,
                    decided: BTreeMap::new(),
                    delivered_wokens: BTreeSet::new(),
                    host_asks: BTreeMap::new(),
                    idem: BTreeMap::new(),
                    budget_ceiling: BTreeMap::new(),
                    leaf_arm,
                }
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
            delivered_wokens: rt.delivered_wokens,
            host_asks: rt.host_asks,
            idem: rt.idem,
            budget_ceiling: rt.budget_ceiling,
            leaf_arm: rt.leaf_arm,
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
        // `max_in_flight_sessions` bounds *live* sessions — a fenced or
        // detached session is residue, not in-flight (the negotiated
        // cap would otherwise deadlock the parked-run workflow: a
        // detached-CLI writer plus the next invocation's session).
        let live = self
            .sessions
            .values()
            .filter(|s| s.detached.is_none())
            .count();
        if live >= self.session_cap() {
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
            delivered_wokens: BTreeSet::new(),
            host_asks: BTreeMap::new(),
            idem: BTreeMap::new(),
            budget_ceiling: BTreeMap::new(),
            leaf_arm: LeafArm::default(),
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
            // The run's event stream seals at `lifecycle.run.finished`
            // — either before this `close` or via the interrupt drive
            // above. A post-terminal `lease.released` row would break
            // the stream ≡ export byte-identity (AC-R-2.11.1-3), so a
            // sealed run releases silently (the lease file still
            // records `Released`).
            let sealed = self
                .store
                .events(&run_id)
                .map(|e| e.iter().any(|ev| ev.class == "lifecycle.run.finished"))
                .unwrap_or(false);
            if sealed {
                self.store.release_silent(&lease).map_err(ledger_err)?;
            } else {
                self.store.release(&lease, &p.reason).map_err(ledger_err)?;
            }
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
    /// The run's persisted sealed definition (DF-S1.25-3; S2.3). Open
    /// deposits `artifacts/<version_id>` — a redirect to the blob-pool
    /// address of the sealed document's canonical bytes — so the
    /// definition survives a service restart. The read verifies the blob
    /// hash (CC3), then the document's own pinned root `version_id`
    /// against the requested address.
    fn persisted_definition(
        &self,
        version_id: &str,
    ) -> Result<hh_hir::document::SealedDefinition, EmbedError> {
        let bytes = self.artifact_bytes(version_id)?;
        let document = hh_hir::document::parse_document(&bytes).map_err(|e| {
            EmbedError::InvalidDefinition {
                diagnostics: vec![format!("persisted definition unreadable: {e}")],
            }
        })?;
        let root =
            document
                .node(&document.root.semantic_id)
                .ok_or_else(|| EmbedError::Refused {
                    reason: "persisted_definition_root_missing".to_string(),
                })?;
        if root.version_id() != version_id {
            return Err(EmbedError::Refused {
                reason: "persisted_definition_corrupt".to_string(),
            });
        }
        Ok(hh_hir::document::SealedDefinition {
            definition_ref: hh_hir::document::DefinitionVersionRef {
                semantic_id: root.semantic_id(),
                version_id: root.version_id(),
            },
            closed_world_tools: hh_hir::closed_world_tools(&document),
            document,
        })
    }

    /// The durable artifact read — `artifacts/<address>` holds the
    /// blob-pool id the bytes were deposited under; `get_blob` verifies
    /// the content hash (CC3). `None` when the artifact was never
    /// deposited (or was collected).
    pub(crate) fn artifact_bytes(&self, address: &str) -> Result<Vec<u8>, EmbedError> {
        let redirect = self.store.root().join("artifacts").join(address);
        let blob_id = std::fs::read_to_string(&redirect).map_err(|_| EmbedError::Refused {
            reason: "artifact_unavailable".to_string(),
        })?;
        let parsed =
            hh_identity::idp::parse_id(blob_id.trim()).map_err(|_| EmbedError::Refused {
                reason: "artifact_corrupt".to_string(),
            })?;
        let addr = hh_identity::idp::ContentAddress {
            idp: "idp/1",
            algorithm: "sha256",
            digest: parsed.digest_hex,
            media_type: String::new(),
            size: 0,
        };
        self.store.get_blob(&addr).map_err(|e| match e {
            hh_ledger::errors::LedgerError::Missing { .. } => EmbedError::Refused {
                reason: "artifact_unavailable".to_string(),
            },
            other => ledger_err(other),
        })
    }

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
        // DF-S1.25-3 (S2.3): the sealed bytes land in the blob pool and
        // `artifacts/<version_id>` records the blob id — `get_artifact`
        // and resume's `verify_resume` survive a service restart (the
        // in-memory table is a cache, never the store). A failure refuses
        // the open (CC3 — the durable half is the point).
        let blob_addr = self
            .store
            .put_blob(&sealed.canonical_bytes(), "application/x-hir-sealed")
            .map_err(ledger_err)?;
        let dir = self.store.root().join("artifacts");
        std::fs::create_dir_all(&dir).map_err(|e| EmbedError::Refused {
            reason: format!("artifact_store_unavailable: {e}"),
        })?;
        let dst = dir.join(&sealed.definition_ref.version_id);
        if !dst.exists() {
            let tmp = dir.join(format!("{}.tmp", sealed.definition_ref.version_id));
            std::fs::write(&tmp, blob_addr.id()).map_err(|e| EmbedError::Refused {
                reason: format!("artifact_store_unavailable: {e}"),
            })?;
            std::fs::rename(&tmp, &dst).map_err(|e| EmbedError::Refused {
                reason: format!("artifact_store_unavailable: {e}"),
            })?;
        }
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
        budget: Option<&BudgetInput>,
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
        // ADR-0168 D6 — `interactive` attendance escalates on every
        // budgeted ceiling; anything else stops `budget_exhausted`.
        let interactive = manifest.attendance.0 == AttendanceValue::Interactive;
        let mut policy = EnvelopePolicy::stage1_default(
            &manifest.budget.clone().unwrap_or_else(|| "b-1".to_string()),
        );
        let (mut budget_ceiling, mut remaining) = budget_dimensions(budget);
        if interactive {
            for dim in budget_ceiling.keys().chain(remaining.keys()) {
                if hh_ontology::dimensions::DimensionId::parse(dim).is_some() {
                    policy.exhaustion.rules.insert(
                        dim.clone(),
                        hh_control::policy::ExhaustionRule {
                            on_exhaustion: hh_control::policy::ExhaustionAction::Escalate,
                            grace_calls: 0,
                        },
                    );
                }
            }
        }
        let policy = policy.seal().map_err(|e| EmbedError::Refused {
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
                budget_ceiling: std::mem::take(&mut budget_ceiling),
                remaining: std::mem::take(&mut remaining),
                interactive_attendance: interactive,
                ..DriverConfig::default()
            },
        )
        .map_err(|e| EmbedError::Refused {
            reason: format!("driver_open: {e:?}"),
        })
    }

    /// Arm the resumed leaf (S2.3; DF-S1.25-2) — the same `ctx`/`policy`/
    /// `config` `arm_driver` builds, but sourced from the persisted
    /// `leaf.arm` record (no `budget` param survives a service restart)
    /// and through `Driver::resume_react` — the durable tail is observed
    /// past `last_cue_seq`, never re-minted. Returns the driver plus the
    /// arm record (the session carries it as `leaf_arm`).
    fn arm_resume_driver(
        &mut self,
        run_id: &str,
        lease: &hh_ledger::store::Lease,
        checkpoint: &[u8],
        manifest: &RunManifest,
    ) -> Result<(Driver<ReactMinimal>, LeafArm), EmbedError> {
        let arm_path = self.store.root().join("runs").join(run_id).join("leaf.arm");
        let arm = std::fs::read_to_string(&arm_path)
            .ok()
            .and_then(|s| hh_wire::json::parse(&s).ok())
            .and_then(|j| LeafArm::from_json(&j))
            .ok_or_else(|| EmbedError::Refused {
                reason: "resume_arm_record_unavailable".to_string(),
            })?;
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
            capabilities_available: arm.surfaces.iter().map(|s| s.surface_id.clone()).collect(),
            steering: (SteerMode::Unsupported, ConcurrentInput::QueueOnly),
        };
        let interactive = manifest.attendance.0 == AttendanceValue::Interactive;
        let mut policy = EnvelopePolicy::stage1_default(
            &manifest.budget.clone().unwrap_or_else(|| "b-1".to_string()),
        );
        if interactive {
            for dim in arm.budget_ceiling.keys().chain(arm.remaining.keys()) {
                if hh_ontology::dimensions::DimensionId::parse(dim).is_some() {
                    policy.exhaustion.rules.insert(
                        dim.clone(),
                        hh_control::policy::ExhaustionRule {
                            on_exhaustion: hh_control::policy::ExhaustionAction::Escalate,
                            grace_calls: 0,
                        },
                    );
                }
            }
        }
        let policy = policy.seal().map_err(|e| EmbedError::Refused {
            reason: format!("envelope_policy: {e:?}"),
        })?;
        let mut sink = crate::runtime::KernelSink {
            store: &mut self.store,
            run_id: run_id.to_string(),
            lease: lease.clone(),
        };
        let mut ceiling = arm.budget_ceiling.clone();
        let mut remaining = arm.remaining.clone();
        let driver = Driver::resume_react(
            &ctx,
            policy,
            checkpoint,
            &mut sink,
            DriverConfig {
                surfaces: arm.surfaces.clone(),
                budget_ceiling: std::mem::take(&mut ceiling),
                remaining: std::mem::take(&mut remaining),
                interactive_attendance: interactive,
                ..DriverConfig::default()
            },
        )
        .map_err(|e| EmbedError::Refused {
            reason: format!("driver_resume: {e:?}"),
        })?;
        Ok((driver, arm))
    }

    /// Persist the resume-by-leaf pair (S2.3) — `leaf.checkpoint` (the
    /// canonical strategy bytes) + `leaf.arm` (the arm-time config) under
    /// `runs/<run_id>/`. Atomic tmp+rename per file: a crash mid-write
    /// leaves the previous generation intact — a torn checkpoint never
    /// reads as a valid one.
    pub(crate) fn persist_leaf(&mut self, sess_id: &str) {
        let (run_id, ckpt, arm) = {
            let s = match self.sessions.get(sess_id) {
                Some(s) => s,
                None => return,
            };
            let d = match s.driver.as_ref() {
                Some(d) => d,
                None => return,
            };
            (s.run_id.clone(), d.checkpoint(), s.leaf_arm.clone())
        };
        let dir = self.store.root().join("runs").join(&run_id);
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let write_atomic = |name: &str, bytes: &[u8]| {
            let tmp = dir.join(format!("{name}.tmp"));
            let dst = dir.join(name);
            if std::fs::write(&tmp, bytes).is_ok() {
                let _ = std::fs::rename(&tmp, &dst);
            }
        };
        write_atomic("leaf.checkpoint", &ckpt);
        write_atomic("leaf.arm", arm.to_json().to_canonical_string().as_bytes());
    }

    fn alloc_session_id(&mut self) -> String {
        self.alloc("sess")
    }

    pub(crate) fn workspace_root(&self) -> &std::path::Path {
        &self.workspace_root
    }
}

// ── module helpers ──────────────────────────────────────────────────────────

/// The declared `dimensions` of a `BudgetInput` split into the
/// driver-counted ceilings (`model_calls`, `turns`, `retries`,
/// `time.working_ms`, `time.wall_ms` — the driver's own consumption fold)
/// and the caller-maintained `remaining` view (token/effect dimensions the
/// boundary does not meter at Stage 1 — their `remaining` stays at the
/// declared ceiling until a metered charge lands, so an unmetered
/// dimension never silently exhausts).
fn budget_dimensions(
    budget: Option<&BudgetInput>,
) -> (BTreeMap<String, i64>, BTreeMap<String, i64>) {
    const DRIVER_COUNTED: &[&str] = &[
        "model_calls",
        "turns",
        "retries",
        "time.working_ms",
        "time.wall_ms",
    ];
    let mut ceiling = BTreeMap::new();
    let mut remaining = BTreeMap::new();
    let dims = match budget {
        Some(BudgetInput::Node(n)) => n
            .get("semantic")
            .and_then(|s| s.get("dimensions"))
            .or_else(|| n.get("dimensions")),
        _ => None,
    };
    if let Some(Json::Obj(m)) = dims {
        for (dim, bound) in m {
            if let Some(hard) = bound.get("hard").and_then(Json::as_int) {
                if DRIVER_COUNTED.contains(&dim.as_str()) {
                    ceiling.insert(dim.clone(), hard);
                } else {
                    remaining.insert(dim.clone(), hard);
                }
            }
        }
    }
    (ceiling, remaining)
}

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
