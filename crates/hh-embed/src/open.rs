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
use hh_control::strategy::{
    ConcurrentInput, ControlContext, ControlStrategy, SteerMode, StrategyParams,
};
use hh_embed_schema::errors::EmbedError;
use hh_embed_schema::types::*;
use hh_env::driver::EnvDriver;
use hh_env::handle::{OnLoss, Roots};
use hh_env::record::{EnvironmentClass, EnvironmentRecord, ImageRef, ProvisioningRecipe};
use hh_hir::records::{AgentProcessBody, KindRecord, SlotBindings};
use hh_identity::names::ResolveMode;
use hh_ledger::manifest::{AttendanceSource, AttendanceValue, RunKind, RunManifest};
use hh_ledger::store::Store;
use hh_wire::json::Json;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// The live control runtime a `resume` carries from the fenced
/// predecessor into the taking-over session — the driver leaf plus the
/// turn flags and pending-ask/idempotency tables (takeover continues the
/// same leaf under the new writer, so its pending asks stay answerable
/// and its idempotency keys still replay).
struct CarriedRuntime {
    driver: Driver<Box<dyn ControlStrategy>>,
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
    /// The bound `control_strategy` slot's variant id — a durable resume
    /// re-arms the same variant (`checkpoint`'s `variant_ref` validates
    /// against it — restore-by-leaf never silently swaps strategies).
    pub control_variant: String,
    /// The bound `compute_policy` slot's variant spelling (§5e.4 — a
    /// resume re-arms the same scheduler; `static` when absent/unbound).
    pub compute_variant: String,
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
            ("control_variant", Json::str(self.control_variant.clone())),
            ("compute_variant", Json::str(self.compute_variant.clone())),
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
            // Pre-S2.11 `leaf.arm` records lack the member — the Stage-1
            // default interpreter is the honest decode (the checkpoint's
            // `variant_ref` would refuse a mismatching restore anyway).
            control_variant: j
                .get("control_variant")
                .and_then(Json::as_str)
                .unwrap_or(crate::open::REACT_MINIMAL_VARIANT)
                .to_string(),
            // Pre-S4.7 `leaf.arm` records lack the member — `static` is
            // the honest decode (no scheduler ⇒ no compute rows).
            compute_variant: j
                .get("compute_variant")
                .and_then(Json::as_str)
                .unwrap_or("static")
                .to_string(),
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
            Replay::AttachOf(run_id) => {
                return self.open_attach(&run_id, p.client.as_ref(), p.contract_json.as_ref())
            }
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
                workspace_trust,
                narrowing_leaves,
                spawn_event,
                notification_sink,
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
                workspace_trust.as_deref(),
                narrowing_leaves,
                spawn_event.as_ref(),
                notification_sink.as_deref(),
                p.client.as_ref(),
                p.contract_json.as_ref(),
                p.invocation.as_ref(),
            )?,
            OpenSpec::Resume {
                run_id,
                mode,
                definition,
                cause,
                ..
            } => self.open_resume(
                run_id,
                mode,
                definition.as_ref(),
                p.invocation.as_ref(),
                cause.as_deref(),
            )?,
            OpenSpec::Attach { run_id } => {
                self.open_attach(run_id, p.client.as_ref(), p.contract_json.as_ref())?
            }
        };
        // S4.10 (§7.2 P8): a declared `client` rides on every opened
        // session — the delivery mint + responder stamping key off it.
        if p.client.is_some() || p.contract_json.is_some() {
            if let Some(sid) = session.get("session_id").and_then(Json::as_str) {
                if let Some(s) = self.sessions.get_mut(sid) {
                    if p.client.is_some() {
                        s.client = p.client.clone();
                    }
                    if p.contract_json.is_some() {
                        s.contract_json = p.contract_json.clone();
                    }
                }
            }
        }
        self.open_idem
            .insert(p.idempotency_key.clone(), session.clone());
        // §7.1 D-2 — an `open_session` that minted deferred asks delivers
        // them to the declared `notification_sink` before returning (the
        // sink file is a derived view of the durable pending rows).
        if let Some(sid) = session.get("session_id").and_then(Json::as_str) {
            self.flush_notification_sink(sid)?;
        }
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

    /// The realized `ModelRoleTable` for a run over the scripted kernel
    /// port (AC-R-2.3.3-11; CF-313): `primary` is the definition's
    /// pinned `native.profile` coordinate unless the caller's
    /// `profile_binding` member names a `primary` row; every other
    /// declared role binds the scripted kernel model under its declared
    /// `profile_ref`. A role outside the closed `ModelRole` set is a
    /// `SchemaViolation` — never coerced, never silently dropped; an
    /// expired bound profile with no declared `intent_ref` refuses
    /// `expired_without_intent` (the §5b.3 d.6 link rule applied at
    /// open). See [`RealizedRoles`] for the returned members.
    fn realized_role_table(
        &self,
        sealed: &hh_hir::document::SealedDefinition,
        supplied: Option<&Json>,
    ) -> Result<RealizedRoles, EmbedError> {
        // Role → `(declared coordinate, intent_ref)` — the caller's rows
        // first, then the definition pin as the `primary` default.
        let mut declared: BTreeMap<String, (String, Option<String>)> = BTreeMap::new();
        if let Some(Json::Obj(m)) = supplied {
            for (role, value) in m {
                if hh_compiler::profile::ModelRole::parse(role).is_none() {
                    return Err(EmbedError::SchemaViolation {
                        path: format!("spec.profile_binding.{role}"),
                        code: "unknown_role".to_string(),
                    });
                }
                let (coord, intent) =
                    profile_binding_entry(value).ok_or_else(|| EmbedError::SchemaViolation {
                        path: format!("spec.profile_binding.{role}"),
                        code: "not_a_coordinate".to_string(),
                    })?;
                declared.insert(role.clone(), (coord, intent));
            }
        }
        if !declared.contains_key("primary") {
            if let Some(pin) = native_profile_pin(&sealed.document) {
                declared.insert("primary".to_string(), (pin, None));
            }
        }
        let mut table = hh_gateway::router::ModelRoleTable::default();
        let mut expired_rows: Vec<(String, String)> = Vec::new();
        let mut primary_semantic: Option<String> = None;
        let mut primary_record_ref: Option<String> = None;
        for (role, (coord, intent)) in declared {
            // Canonicalise through the registry: the recorded
            // `profile_ref` is the resolved profile's canonical
            // `id@version` (never a substituted guess — a coordinate
            // that doesn't resolve is recorded verbatim: a declared
            // claim, not a resolved record).
            let resolved = crate::bundle_ops::resolve_profile_coordinate(&self.registry, &coord);
            let (profile_ref, semantic, record_ref, expired) = match &resolved {
                Some((vid, p)) => (
                    hh_compiler::profile::profile_coordinate(p),
                    Some(p.content_hash.clone()),
                    Some(vid.clone()),
                    hh_compiler::profile::profile_status(p)
                        == hh_compiler::profile::DebtStatus::Expired,
                ),
                None => (coord.clone(), Some(coord.clone()), None, false),
            };
            if expired {
                match intent {
                    Some(i) => expired_rows.push((profile_ref.clone(), i)),
                    None => {
                        return Err(EmbedError::Refused {
                            reason: format!("expired_without_intent: {profile_ref}"),
                        });
                    }
                }
            }
            if role == "primary" {
                primary_semantic = semantic;
                primary_record_ref = record_ref;
            }
            table.roles.insert(
                role,
                hh_gateway::router::RoleBinding {
                    primary: scripted_route_candidate(&profile_ref),
                    alternates: Vec::new(),
                    policy_ref: "hh-embed/scripted".to_string(),
                    profile_ref,
                },
            );
        }
        Ok(RealizedRoles {
            table,
            primary_identity: primary_semantic,
            primary_record_ref,
            expired_rows,
        })
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
        workspace_trust: Option<&str>,
        narrowing_leaves: &[NarrowingLeaf],
        spawn_event: Option<&hh_embed_schema::types::SpawnEventRef>,
        notification_sink: Option<&str>,
        client: Option<&hh_embed_schema::ClientDecl>,
        contract_json: Option<&Json>,
        invocation: Option<&InvocationRecord>,
    ) -> Result<Json, EmbedError> {
        // §7.1 D-2 notification-sink validation (ADR-0304 D1): the sink
        // is a kernel-side *delivery* declaration — Stage-1 admits only
        // `sink:file:<rel>` (a workspace-relative path the host watches).
        // An absolute or escaping path can never be a workspace-relative
        // delivery target → `SchemaViolation` pre-ledger.
        if let Some(s) = notification_sink {
            if let Some(p) = s.strip_prefix("sink:file:") {
                let bad = p.is_empty()
                    || p.starts_with('/')
                    || p.split('/').any(|seg| seg == ".." || seg.is_empty());
                if bad {
                    return Err(EmbedError::SchemaViolation {
                        path: "spec.notification_sink".to_string(),
                        code: "sink_path_escape".to_string(),
                    });
                }
            } else {
                return Err(EmbedError::SchemaViolation {
                    path: "spec.notification_sink".to_string(),
                    code: "unsupported_sink".to_string(),
                });
            }
        }
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
        // The realized `ModelRoleTable` (AC-R-2.3.3-11; CF-313;
        // ADR-0121 d.2): `primary` is the definition's `native.profile`
        // pin unless the caller's `profile_binding` names a `primary`
        // row; every other declared role binds the scripted kernel model
        // under its own `profile_ref`. The table's `semantic_id` is the
        // `configuration_id.model_ref` (two spec blocks differing only
        // in the table differ in the configuration id — the AC's
        // determinism leg), and its profile projection is the manifest's
        // `profile_binding` member. A bound profile that resolves
        // `expired` requires the row's declared `intent_ref` (the §5b.3
        // d.6 link rule applied at open; `expired_without_intent` is the
        // refusal) — each admitted row mints `model.profile.expired_used`
        // once the run exists.
        let RealizedRoles {
            table: role_table,
            primary_identity: primary_semantic,
            primary_record_ref,
            expired_rows,
        } = self.realized_role_table(&sealed, profile_binding)?;
        // The realized configuration pair — content-addressed through
        // `hh_assembly::configuration` (CC9): the manifest pins the
        // honest Stage-1 composition inputs (the realized role table's
        // semantic id, the primary profile's identity, the realized
        // environment, the declared budget, seed `0` — `open_session`
        // carries no seed at Stage 1). The ledger's `open_run` gate
        // requires pinned ids.
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
                model_ref: role_table.semantic_id(),
                profile: primary_semantic.unwrap_or_else(|| "profile:none".to_string()),
                environment_ref: "local_host".to_string(),
                budget: budget_input.clone(),
                seed: "0".to_string(),
            },
        );
        let configuration_id = cfg.configuration_id;
        let configuration_version_id = cfg.configuration_version_id;

        // ── environment spec + the `BypassWithoutContainment` gate ─────
        // The spec is pure — building it pre-`open_run` lets the
        // manifest record the binding's enforcement evidence
        // (`containment.enforcement_evidence`, §7.1 §3's consumed-fields
        // row) and lets a `bypass` approval mode fail *before any run
        // opens* when the binding cannot mint it (AC-R-2.11.1-7). The
        // gate runs the identical apply+battery fold the real attach
        // will run — never a surface claim (`attach::evidence_preview`).
        let env_spec = {
            let ws = self.workspace_root().display().to_string();
            environment_spec(environment, &ws)?
        };
        let (_, _, env_policy, _, env_attestation) = &env_spec;
        // S4.14b (R-2.8.4¹) — the backend is *selected* by the policy's
        // `proc.isolation_class` (`for_policy`), never hard-coded to the
        // EP2 model; attesting classes need the binding's declared
        // `substrate_attestation` (`attestation_missing` refuses).
        let backend = hh_containment::backend::for_policy(env_policy, env_attestation.clone())
            .map_err(|e| EmbedError::EnvironmentUnavailable {
                reason: format!("containment_backend:{e:?}"),
            })?;
        let evidence =
            hh_containment::attach::evidence_preview(&*backend, env_policy).map_err(|e| {
                EmbedError::EnvironmentUnavailable {
                    reason: format!("containment_preview:{e:?}"),
                }
            })?;
        if approval_mode == Some("bypass") {
            bypass_gate(env_policy, env_attestation.clone())?;
        }

        // ── manifest + open_run ──────────────────────────────────────
        let mut manifest = RunManifest::minimal(RunKind::Agent);
        // `workspace_trust` — the H5 trust-store claim recorded verbatim
        // (ADR-0168 D7); absent ⇒ `unknown`, never inferred (OQ-387).
        manifest.workspace_trust = workspace_trust
            .and_then(hh_ledger::manifest::WorkspaceTrust::parse)
            .unwrap_or(hh_ledger::manifest::WorkspaceTrust::Unknown);
        // `containment.enforcement_evidence` — the environment handle's
        // evidence the gate computed (kernel-minted, never CLI-asserted).
        let bypass_admissible = hh_containment::attach::relied_groups(env_policy)
            .iter()
            .all(|g| {
                evidence
                    .get(g)
                    .is_some_and(|e| *e != hh_containment::report::EnforcementEvidence::Unknown)
            });
        manifest.extra.insert(
            "containment".to_string(),
            Json::obj([
                ("enforcement_evidence", evidence_json(&evidence)),
                ("backend", Json::str(backend.name())),
                // The relied-groups verdict the pre-open gate computed —
                // `amend(approval_mode → bypass)` re-reads *this* durable
                // record, never re-runs the probe battery.
                ("bypass_admissible", Json::Bool(bypass_admissible)),
            ]),
        );
        // The approval mode the run opened under — a manifest claim the
        // resume path re-reads so a later session's `policy_mode` (and
        // `amend`'s widening check) reflects the record, never a guess.
        if let Some(am) = approval_mode {
            manifest
                .extra
                .insert("approval_mode".to_string(), Json::str(am));
        }
        // §7.1 D-2 — the declared `notification_sink` is a manifest claim
        // (`manifest.extra["notification_sink"]`) every later session
        // re-reads for delivery: the durable pending rows are the
        // authority, the sink is a derived view (ADR-0304 D1).
        if let Some(s) = notification_sink {
            manifest
                .extra
                .insert("notification_sink".to_string(), Json::str(s));
        }
        // The declared Π narrowing leaves (surface presets — ADR-0168 D3):
        // their ids are a `policy_fingerprint` leg, so the manifest records
        // the id set the fingerprint folded (leaf change ⇒ lease revocation
        // by construction, ADR-0071 D1).
        if !narrowing_leaves.is_empty() {
            manifest.extra.insert(
                "narrowing_leaves".to_string(),
                Json::Arr(
                    narrowing_leaves
                        .iter()
                        .map(|l| Json::str(l.leaf_id()))
                        .collect(),
                ),
            );
        }
        // S3.10 — the `TaskContract` projection stamp (§5f.2): the sealed
        // `Goal` projects at open; `manifest.extra["task_contract_id"]`
        // carries the `contract_id` (the gate's pass-table identity).
        if let Some(contract) = project_task_contract(&sealed.document) {
            manifest.extra.insert(
                "task_contract_id".to_string(),
                Json::str(contract.contract_id.clone()),
            );
        }
        manifest.extra.insert(
            "control_variant".to_string(),
            Json::str(
                control_slot_variant(&sealed.document)
                    .unwrap_or_else(|| REACT_MINIMAL_VARIANT.to_string()),
            ),
        );
        // §5e.4 — the `compute_policy` slot's bound variant stamps the
        // manifest (`static` semantics when the slot is unbound — the
        // member stays absent, the run is byte-identical to pre-S4.7).
        manifest.compute_policy_ref = compute_slot_variant(&sealed.document);
        // The run manifest's `profile_binding` member (CF-313;
        // AC-R-2.3.3-11): the profile projection of the realized
        // `ModelRoleTable` — `{roles: map<ModelRole, ProfileRef>,
        // fallback_used}`. `fallback_used` is false here — the scripted
        // port never falls back silently (a missing binding is `unbound`,
        // never substituted).
        manifest.extra.insert(
            "profile_binding".to_string(),
            Json::obj([
                ("roles", role_table.profile_binding()),
                ("fallback_used", Json::Bool(false)),
            ]),
        );
        // `model_profile_ref` — the resolved record's `version_id` for
        // the `primary` binding (absent when `primary` binds no
        // resolvable record: the ledger's `open_run` gate admits only a
        // ref that resolves — the declared coordinate stays legible on
        // `profile_binding.roles.primary.profile_ref`, never widened).
        manifest.model_profile_ref = primary_record_ref;
        // `expired_used` — the `{profile_ref, intent_ref}` rows the run
        // opens under (the manifest's record of the
        // `model.profile.expired_used` events minted below; absent when
        // the run binds no expired profile).
        if !expired_rows.is_empty() {
            manifest.extra.insert(
                "expired_used".to_string(),
                Json::Arr(
                    expired_rows
                        .iter()
                        .map(|(p, i)| {
                            Json::obj([
                                ("profile_ref", Json::str(p.clone())),
                                ("intent_ref", Json::str(i.clone())),
                            ])
                        })
                        .collect(),
                ),
            );
        }
        manifest.configuration_id = Some(configuration_id.clone());
        manifest.configuration_version_id = Some(configuration_version_id.clone());
        manifest.harness_def_ref = Some(manifest_ref.clone());
        // §6.2's one-snapshot rule: the manifest pins the registry snapshot
        // the definition resolved against (§3.3.6 `resolved.registry_snapshot_id`)
        // so `kernel.bundle` emits it on every bundle, experiment-bound or not.
        manifest.registry_snapshot_id = sealed
            .document
            .assembly
            .as_ref()
            .and_then(|a| a.get("resolved"))
            .and_then(|r| r.get("registry_snapshot_id"))
            .and_then(Json::as_str)
            .map(str::to_string);
        manifest.attendance = (
            AttendanceValue::parse(&attendance.value).unwrap_or(AttendanceValue::Async),
            AttendanceSource::parse(&attendance.source).unwrap_or(AttendanceSource::Declared),
        );
        manifest.budget = budget.map(|_| budget_input);
        manifest.overrides_layer_id = overrides_layer_id.clone();
        // S4.11 — the surface-declared launch-causality citation
        // (R-2.11.3¹; ADR-0303 D5): `open_run` resolves the ref against
        // the durable prefix (`UnresolvedEventRef` on a dangling claim);
        // the child's `lifecycle.run.created` then carries the
        // `causes[]` record §7.3 requires — the `spawn_event` member is
        // the ledger's own launch-causality carrier (one scheme, CC1).
        if let Some(se) = spawn_event {
            manifest.spawn_event = Some(hh_ledger::manifest::EventRef {
                run_id: se.run_id.clone(),
                event_id: se.event_id.clone(),
            });
        }
        let holder = self.holder.clone();
        let (run_id, lease) = self
            .store
            .open_run(manifest.clone(), &holder)
            .map_err(ledger_err)?;

        // ── pre-authorization handles (§5g.1 §9 Stage-2; ADR-0053 D5) ──
        // Every sealed `HarnessRule{pre_authorize}` mints a `policy_rule`-
        // basis `AuthorityHandle` over the rule's declared grants at open —
        // the durable `security.permission.granted` rows are what
        // `authorize`'s `pre_authorized` leg and the reviewer chain's
        // `policy_rule` stage read on rebuild (raise-only admission: the
        // handles narrow-or-satisfy, never widen). A rule whose `grants`
        // don't decode skips (the sealed definition validated them — mint
        // never guesses).
        {
            let issuer = hh_provenance::ProvenanceRecord::kernel(
                "hh-embed/open_session",
                self.store.now_ms(),
            );
            let holder_ref = hh_hir::refs::Ref::selected(holder.clone(), "latest");
            let minted = {
                let store = &self.store;
                let mut alloc = |kind: &str| store.alloc_id(kind);
                hh_monitor::mint::mint_preauthorization_handles(
                    &sealed,
                    &holder_ref,
                    &issuer,
                    &run_id,
                    &mut alloc,
                )
            };
            for (handle, granted_event_id) in minted {
                let ev = hh_env::events::EventMinter::new(&self.store, &run_id)
                    .mint_with_id(
                        "security.permission.granted",
                        hh_monitor::events::granted_payload(&handle),
                        granted_event_id,
                    )
                    .map_err(ledger_err)?;
                self.store
                    .append(&run_id, &lease, vec![ev])
                    .map_err(ledger_err)?;
            }
        }

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

        // `model.profile.expired_used{profile_ref, intent_ref}` — the
        // §3.3.1 ledger event for each role binding that admitted an
        // expired profile under its declared intent (R-2.3.3;
        // AC-R-2.3.3-11). The manifest's `expired_used` member records
        // the same rows; an expired binding with no `intent_ref`
        // refused at `realized_role_table` — never minted.
        for (profile_ref, intent_ref) in &expired_rows {
            self.mint(
                &run_id,
                &lease,
                "model.profile.expired_used",
                hh_gateway::events::profile_expired_used(profile_ref, intent_ref),
            )?;
        }

        // ── environment: provision + attach local_host ───────────────
        let (env_handle_id, env_json) = self.provision_environment(&run_id, &lease, environment)?;

        // ── session + driver ─────────────────────────────────────────
        let session_id = self.alloc_session_id();
        let mut attached_payload = Json::obj([
            ("session_id", Json::str(session_id.clone())),
            ("mode", Json::str("new")),
            ("attachment_id", Json::str(session_id.clone())),
        ]);
        if let Json::Obj(m) = &mut attached_payload {
            if let Some(c) = client {
                m.insert("client".into(), c.to_json());
            }
            if let Some(ct) = contract_json {
                m.insert("contract_json".into(), ct.clone());
            }
        }
        self.mint(
            &run_id,
            &lease,
            "lifecycle.session.attached",
            attached_payload,
        )?;

        let surfaces = driver_surfaces(&cap_decl);
        let control_variant = control_slot_variant(&sealed.document)
            .unwrap_or_else(|| REACT_MINIMAL_VARIANT.to_string());
        let compute_variant =
            compute_slot_variant(&sealed.document).unwrap_or_else(|| "static".to_string());
        let driver = self.arm_driver(
            &run_id,
            &lease,
            &surfaces,
            &manifest,
            budget,
            &control_variant,
            &compute_variant,
            Some(&sealed.document),
        )?;
        let realized = realized_settings(
            self.workspace_root(),
            attendance,
            approval_mode,
            Some(&role_table),
        );
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
            authority_caps: hh_monitor::mint::cap_rows(&sealed.document),
            narrowing_leaf_ids: narrowing_leaves.iter().map(|l| l.leaf_id()).collect(),
            pendings: BTreeMap::new(),
            decided: BTreeMap::new(),
            delivered_wokens: BTreeSet::new(),
            host_asks: BTreeMap::new(),
            idem: BTreeMap::new(),
            budget_ceiling: budget_dimensions(budget).0,
            steering: steering_for(&control_variant),
            pending_steer: None,
            leaf_arm: LeafArm {
                surfaces: surfaces.clone(),
                budget_ceiling: budget_dimensions(budget).0,
                remaining: budget_dimensions(budget).1,
                control_variant: control_variant.clone(),
                compute_variant: compute_slot_variant(&sealed.document)
                    .unwrap_or_else(|| "static".to_string()),
            },
            scan_seq: head.seq,
            next_invoke: None,
            next_completion: String::new(),
            next_response_ref: String::new(),
            client: client.cloned(),
            contract_json: contract_json.cloned(),
            sink_seq: head.seq as i64,
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
        // AC-R-2.11.1-7's bypass split (I-P1): under `approval_mode = bypass`
        // (admitted only with recorded `enforcement_evidence` — `bypass_gate`
        // above), an ask *outside* the constitutional never-auto set
        // auto-resolves `decided{decision: allow, decider: policy,
        // reason: bypass}` at either attendance; a never-auto ask
        // (irreversible or unknown — undeclared `risk_class` reads UNKNOWN,
        // ADR-0031 §2) still reaches the human stage attended and still
        // denies unattended (Π-12's deny covers it below).
        let bypass = approval_mode == Some("bypass");
        for cap in cap_decl.iter().filter(|c| c.requires_approval) {
            let proposal = format!("host capability {} requests approval", cap.capability_id);
            let never_auto = hh_monitor::approval::never_auto(
                cap.risk_class
                    .unwrap_or(hh_ontology::risk::RiskClass::UNKNOWN),
            );
            if bypass && !never_auto {
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
                        ("decision", Json::str("allow")),
                        ("decider", Json::str("policy")),
                        ("reason", Json::str("bypass")),
                    ]),
                )?;
            } else if unattended {
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
        cause: Option<&str>,
    ) -> Result<Json, EmbedError> {
        // S5.8 (AC-R-2.2.3-13) — the `lifecycle.run.resumed` row's
        // `recovery_decision.cause` is the *caller's* claim over the
        // closed set; absent ⇒ `operator` (the pre-S5.8 spelling).
        let cause = cause.unwrap_or("operator");
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
                .restore_caused(run_id, &holder, LEASE_TTL_MS, cause)
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
                                    requested_grants: crate::service::decode_requested_grants(&req),
                                    deadline_ms: e
                                        .payload
                                        .get("timeout")
                                        .and_then(Json::as_int)
                                        .map(|t| {
                                            (e.payload
                                                .get("requested_at")
                                                .and_then(Json::as_int)
                                                .unwrap_or(0)
                                                .max(0)
                                                as u64)
                                                .saturating_add(t.max(0) as u64)
                                        }),
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
        // Rebuild realized settings from the manifest's recorded claims —
        // the run's *own* attendance and approval mode (never a fresh
        // default: a resumed session's `policy_mode` feeds Π decisions
        // and `amend`'s widening check — ADR-0168 D1/D3).
        let manifest_attendance = AttendanceDeclaration {
            value: manifest.attendance.0.as_str().to_string(),
            source: manifest.attendance.1.as_str().to_string(),
        };
        let manifest_mode = manifest.extra.get("approval_mode").and_then(Json::as_str);
        let realized = realized_settings(
            self.workspace_root(),
            &manifest_attendance,
            manifest_mode,
            None,
        );
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
            steering: steering_for(&rt.leaf_arm.control_variant),
            pending_steer: None,
            env_json: rt.env_json,
            env_handle_id: rt.env_handle_id,
            host_caps: rt.host_caps,
            turn_active: rt.turn_active,
            active_turn: rt.active_turn,
            finished: false,
            detached: None,
            // The caps re-derive from the persisted sealed definition — the
            // manifest pins `harness_def_ref`; a missing artifact fails
            // closed to `[]` (uncapped at the response's own authority —
            // the mint's grants are still only the recorded `requested`).
            authority_caps: manifest
                .harness_def_ref
                .as_deref()
                .and_then(|r| self.persisted_definition(r).ok())
                .map(|d| hh_monitor::mint::cap_rows(&d.document))
                .unwrap_or_default(),
            // The leaf ids re-derive from the manifest's durable
            // `narrowing_leaves` member — the fingerprint leg survives
            // a host restart (ADR-0168 D3; absent ⇒ `[]`).
            narrowing_leaf_ids: manifest_leaf_ids(&manifest),
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
            client: None,
            contract_json: None,
            sink_seq: head.seq as i64,
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
    /// `open_session{attach}` — the read-only session (no lease, no driver,
    /// no append path). A declared `client{kind}` mints
    /// `lifecycle.session.attached{binding, client{kind}}` on the subject
    /// run — the durable surface-session record §7.2 P12/§6.6 require and
    /// the V10 accountability view reads (ADR-0301 D2; kernel-origin row via
    /// `commit_kernel_row_for` — an attach session holds no lease). A
    /// declared `contract_json` rides the same row (AC-R-2.11.4-15).
    fn open_attach(
        &mut self,
        run_id: &str,
        client: Option<&hh_embed_schema::ClientDecl>,
        contract_json: Option<&Json>,
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
        self.store.events(run_id).map_err(ledger_err)?;
        let manifest = self.store.manifest(run_id).map_err(ledger_err)?.clone();
        let head = self.store.head(run_id).map_err(ledger_err)?;
        let session_id = self.alloc_session_id();
        // `describe` reports the run's *recorded* settings — the
        // manifest's attendance + approval_mode claims — not a fresh
        // attach-time default (ADR-0168 D1/D3).
        let realized = realized_settings(
            self.workspace_root(),
            &AttendanceDeclaration {
                value: manifest.attendance.0.as_str().to_string(),
                source: manifest.attendance.1.as_str().to_string(),
            },
            manifest.extra.get("approval_mode").and_then(Json::as_str),
            None,
        );
        let sess = SessionState {
            run_id: run_id.to_string(),
            attach: true,
            lease: None,
            manifest_ref: sess_manifest_ref(&manifest),
            realized: realized.clone(),
            driver: None,
            steering: (SteerMode::Unsupported, ConcurrentInput::QueueOnly),
            pending_steer: None,
            env_json: Json::Null,
            env_handle_id: None,
            host_caps: Vec::new(),
            turn_active: false,
            active_turn: "turn-1".to_string(),
            finished: false,
            detached: None,
            authority_caps: Vec::new(),
            narrowing_leaf_ids: Vec::new(),
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
            client: client.cloned(),
            contract_json: contract_json.cloned(),
            sink_seq: head.seq as i64,
        };
        self.sessions.insert(session_id.clone(), sess);
        // The surface-session record (§7.2 §6.6) — minted only for
        // sessions that declare a client so CLI attach reads stay
        // byte-identical (CC8: additive, never a silent shape change).
        // Post-finish attaches append anyway: the `commit_kernel_row_for`
        // path rides the WAL tip like `measurement.export.delivered`
        // (kernel-origin facts, never a client lease).
        if let Some(c) = client {
            let binding = self.binding_label.clone();
            let mut payload = Json::obj([
                ("session_id", Json::str(session_id.clone())),
                ("binding", Json::str(binding)),
                ("client", c.to_json()),
            ]);
            if let Some(ct) = contract_json {
                if let Json::Obj(m) = &mut payload {
                    m.insert("contract_json".into(), ct.clone());
                }
            }
            self.store
                .commit_kernel_row_for(
                    "kernel:surface",
                    run_id,
                    "lifecycle.session.attached",
                    payload,
                    vec![],
                    vec![],
                )
                .map_err(ledger_err)?;
        }
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

    /// §7.1 D-2 — the declared `notification_sink` delivery sweep
    /// (ADR-0304 D1): every durable `security.permission.requested`,
    /// `security.permission.pending`, `security.permission.decided`,
    /// or `lifecycle.run.suspended` row appended since the session's
    /// watermark lands on the run's declared `sink:file:<rel>` as one
    /// canonical JSON line (`{seq, event_id, class, ts, payload}`).
    /// The ledger rows are the authority — the sink file is a derived,
    /// at-least-once notification view a detached host watches; a run
    /// with no declared sink (or a sink other than `sink:file:*`) is a
    /// no-op.
    pub(crate) fn flush_notification_sink(&mut self, session_id: &str) -> Result<(), EmbedError> {
        let Some(s) = self.sessions.get_mut(session_id) else {
            return Ok(());
        };
        let run_id = s.run_id.clone();
        let since = s.sink_seq;
        let manifest = self.store.manifest(&run_id).map_err(ledger_err)?.clone();
        let Some(sink) = manifest
            .extra
            .get("notification_sink")
            .and_then(Json::as_str)
            .map(str::to_string)
        else {
            return Ok(());
        };
        let Some(rel) = sink.strip_prefix("sink:file:").map(str::to_string) else {
            return Ok(());
        };
        let events = self.store.events(&run_id).map_err(ledger_err)?;
        let rows: Vec<Json> = events
            .iter()
            .filter(|e| {
                (e.seq as i64) > since
                    && matches!(
                        e.class.as_str(),
                        "security.permission.requested"
                            | "security.permission.pending"
                            | "security.permission.decided"
                            | "lifecycle.run.suspended"
                    )
            })
            .map(|e| {
                Json::obj([
                    ("seq", Json::Int(e.seq as i64)),
                    ("event_id", Json::str(e.event_id.clone())),
                    ("class", Json::str(e.class.clone())),
                    ("ts", Json::str(e.ts.clone())),
                    ("payload", e.payload.clone()),
                ])
            })
            .collect();
        let watermark = events.last().map(|e| e.seq as i64).unwrap_or(since);
        if rows.is_empty() {
            if let Some(s) = self.sessions.get_mut(session_id) {
                s.sink_seq = watermark;
            }
            return Ok(());
        }
        // Delivery is best-effort: the durable pending/asked rows are the
        // authority, so a sink write failure never fails the dispatch —
        // the watermark only advances past rows actually written (a later
        // session retries the tail, at-least-once).
        let path = self.workspace_root().join(&rel);
        if let Some(parent) = path.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                return Ok(());
            }
        }
        let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        else {
            return Ok(());
        };
        use std::io::Write;
        let mut delivered_upto = since;
        for (i, row) in rows.iter().enumerate() {
            let mut line = row.to_canonical_string();
            line.push('\n');
            if f.write_all(line.as_bytes()).is_err() {
                break;
            }
            if let Json::Int(s) = row.get("seq").cloned().unwrap_or(Json::Int(0)) {
                delivered_upto = s;
            }
            let _ = i;
        }
        if let Some(s) = self.sessions.get_mut(session_id) {
            s.sink_seq = delivered_upto;
        }
        Ok(())
    }

    /// `close{reason}` — the writer's drain (interrupt → finished →
    /// release) or the attach session's detach; the result is the head
    /// coordinate at close (`Closed{final}`).
    pub(crate) fn close(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = CloseParams::from_json(params)?;
        let s = self.live_session(&p.session_id)?;
        let (run_id, attach, lease, client) = (
            s.run_id.clone(),
            s.attach,
            s.lease.clone(),
            s.client.clone(),
        );
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
        } else if let Some(c) = &client {
            // S4.10 (§7.2 §6.6) — the attach-side session record, the
            // declared-client pair of `lifecycle.session.attached`:
            // `detached{session_id, reason, binding, client{kind}}` rides
            // the same kernel-origin path (attach holds no lease).
            let binding = self.binding_label.clone();
            self.store
                .commit_kernel_row_for(
                    "kernel:surface",
                    &run_id,
                    "lifecycle.session.detached",
                    Json::obj([
                        ("session_id", Json::str(p.session_id.clone())),
                        ("reason", Json::str(p.reason.clone())),
                        ("binding", Json::str(binding)),
                        ("client", c.to_json()),
                    ]),
                    vec![],
                    vec![],
                )
                .map_err(ledger_err)?;
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
    pub(crate) fn persisted_definition(
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
            notices: None,
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
        let ws = self.workspace_root().display().to_string();
        let (record, roots, policy, info, attestation) = environment_spec(environment, &ws)?;
        // S4.14b (R-2.8.4¹) — select the backend by the policy's
        // `proc.isolation_class` (`for_policy`); the attesting classes
        // refuse without the binding's declared `substrate_attestation`.
        // Fail-closed: the report is stored and the applied row lands
        // durable.
        let backend = hh_containment::backend::for_policy(&policy, attestation).map_err(|e| {
            EmbedError::EnvironmentUnavailable {
                reason: format!("containment_backend:{e:?}"),
            }
        })?;
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
        driver
            .attach(
                &mut self.store,
                lease,
                &handle.env_handle_id,
                Some(&*backend),
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

    /// Arm the session driver over the session's run — the bound
    /// `control_strategy` slot selects the variant (`strategy_for`; the
    /// canonical control loop under `KernelSink` + the writer lease).
    #[allow(clippy::too_many_arguments)] // the arming record is the §5f.2 arm tuple — the arity is the call's.
    fn arm_driver(
        &mut self,
        run_id: &str,
        lease: &hh_ledger::store::Lease,
        surfaces: &[SurfaceSpec],
        manifest: &RunManifest,
        budget: Option<&BudgetInput>,
        control_variant: &str,
        compute_variant: &str,
        doc: Option<&hh_hir::document::HirDocument>,
    ) -> Result<Driver<Box<dyn ControlStrategy>>, EmbedError> {
        // §5e.4 — the bound `compute_policy` variant must resolve at arm
        // time (`policy_for` is the admission check; a `bandit`/
        // `surface_prior`/`predictor` ref or an unknown spelling refuses
        // the open, never fails mid-run).
        hh_control::compute::policy_for(compute_variant).map_err(|e| EmbedError::Refused {
            reason: format!("compute_policy: {e:?}"),
        })?;
        // S3.10 — the `TaskContract` projection (§5f.2; ADR-0109 D1): the
        // sealed doc's `Goal` projects to the gate's pass table; the
        // `contract_id` is stamped `manifest.extra["task_contract_id"]` at
        // `open_run`.
        let task_contract = doc.and_then(project_task_contract);
        // The sealed `TaskContract`'s `task_value` is the ctx's
        // authoritative value anchor (§5e.4 — a slot param may restate
        // it but never overrides the contract the value was sealed on).
        let mut compute_facts = doc.map(compute_facts_for).unwrap_or_default();
        if let Some(tv) = task_contract.as_ref().and_then(|c| c.task_value.as_ref()) {
            compute_facts.task_value = Some(hh_control::compute::TaskValueFact::from_task_value(
                &task_contract
                    .as_ref()
                    .map(|c| c.contract_id.clone())
                    .unwrap_or_default(),
                tv,
            ));
        }
        // `plan_execute` arms the `hh.plan` surface + the model-emitted
        // switch (S3.10); other variants keep the react preset params.
        let is_plan_execute = control_variant.trim_end_matches("@1") == "hh/plan-execute";
        let mut surfaces = surfaces.to_vec();
        if is_plan_execute
            && !surfaces
                .iter()
                .any(|s| s.surface_id == hh_control::plan_exec::PLAN_SURFACE_ID)
        {
            surfaces.push(SurfaceSpec {
                surface_id: hh_control::plan_exec::PLAN_SURFACE_ID.to_string(),
                semantic_id: "hh/plan-execute/plan-surface".to_string(),
                params: BTreeMap::new(),
            });
        }
        let mut parameters = StrategyParams::default();
        if is_plan_execute {
            parameters.plan_provenance = hh_control::strategy::PlanProvenance::ModelEmitted;
            parameters.max_continue_nudges = 1;
        }
        // The boundary is the *variant's* preset (S3.10) — `plan_execute`
        // assigns `plan`/`delegate` to the model; react keeps its own.
        let boundary = if is_plan_execute {
            hh_control::plan_exec::PlanExecute::new()
                .capabilities()
                .boundary_preset
                .clone()
        } else {
            hh_control::react::react_preset()
        };
        let ctx = ControlContext {
            process_ref: format!("hh-embed/{}", manifest.run_kind.as_str()),
            plan: vec![],
            boundary,
            profile: Json::Null,
            account_ref: manifest
                .budget
                .clone()
                .unwrap_or_else(|| "acct:unbudgeted".to_string()),
            budget_ref: manifest.budget.clone().unwrap_or_else(|| "b-1".to_string()),
            envelope_ref: "env-1".to_string(),
            parameters,
            capabilities_available: surfaces.iter().map(|s| s.surface_id.clone()).collect(),
            steering: steering_for(control_variant),
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
        Driver::open(
            strategy_for(control_variant),
            &ctx,
            policy,
            &mut sink,
            DriverConfig {
                surfaces,
                budget_ceiling: std::mem::take(&mut budget_ceiling),
                remaining: std::mem::take(&mut remaining),
                interactive_attendance: interactive,
                task_contract,
                plan_surface_id: is_plan_execute
                    .then(|| hh_control::plan_exec::PLAN_SURFACE_ID.to_string()),
                compute_policy_ref: compute_variant.to_string(),
                compute_facts,
                compute_rules: doc.and_then(compute_rules_for),
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
    ) -> Result<(Driver<Box<dyn ControlStrategy>>, LeafArm), EmbedError> {
        let arm_path = self.store.root().join("runs").join(run_id).join("leaf.arm");
        let arm = std::fs::read_to_string(&arm_path)
            .ok()
            .and_then(|s| hh_wire::json::parse(&s).ok())
            .and_then(|j| LeafArm::from_json(&j))
            .ok_or_else(|| EmbedError::Refused {
                reason: "resume_arm_record_unavailable".to_string(),
            })?;
        // The resumed leaf re-arms the *variant's* preset + parameters +
        // plan surface (S3.10 — `plan_execute` resumes `model_emitted`);
        // the `TaskContract` re-projects from the persisted sealed
        // definition (`manifest.harness_def_ref`), never from a side file.
        let is_plan_execute = arm.control_variant.trim_end_matches("@1") == "hh/plan-execute";
        let boundary = if is_plan_execute {
            hh_control::plan_exec::PlanExecute::new()
                .capabilities()
                .boundary_preset
                .clone()
        } else {
            hh_control::react::react_preset()
        };
        let mut parameters = StrategyParams::default();
        if is_plan_execute {
            parameters.plan_provenance = hh_control::strategy::PlanProvenance::ModelEmitted;
            parameters.max_continue_nudges = 1;
        }
        let task_contract = manifest
            .harness_def_ref
            .as_deref()
            .and_then(|r| self.persisted_definition(r).ok())
            .and_then(|sealed| project_task_contract(&sealed.document));
        let ctx = ControlContext {
            process_ref: format!("hh-embed/{}", manifest.run_kind.as_str()),
            plan: vec![],
            boundary,
            profile: Json::Null,
            account_ref: manifest
                .budget
                .clone()
                .unwrap_or_else(|| "acct:unbudgeted".to_string()),
            budget_ref: manifest.budget.clone().unwrap_or_else(|| "b-1".to_string()),
            envelope_ref: "env-1".to_string(),
            parameters,
            capabilities_available: arm.surfaces.iter().map(|s| s.surface_id.clone()).collect(),
            steering: steering_for(&arm.control_variant),
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
        // The resume re-projects the scheduler's declared facts from the
        // persisted definition — the same fold `arm_driver` runs (the
        // sealed doc is the arm record, never a side channel).
        let sealed_def = manifest
            .harness_def_ref
            .as_deref()
            .and_then(|r| self.persisted_definition(r).ok());
        let mut compute_facts = sealed_def
            .as_ref()
            .map(|sealed| compute_facts_for(&sealed.document))
            .unwrap_or_default();
        let compute_rules = sealed_def
            .as_ref()
            .and_then(|sealed| compute_rules_for(&sealed.document));
        if let Some(tv) = task_contract.as_ref().and_then(|c| c.task_value.as_ref()) {
            compute_facts.task_value = Some(hh_control::compute::TaskValueFact::from_task_value(
                &task_contract
                    .as_ref()
                    .map(|c| c.contract_id.clone())
                    .unwrap_or_default(),
                tv,
            ));
        }
        let mut sink = crate::runtime::KernelSink {
            store: &mut self.store,
            run_id: run_id.to_string(),
            lease: lease.clone(),
        };
        let mut ceiling = arm.budget_ceiling.clone();
        let mut remaining = arm.remaining.clone();
        let driver = Driver::resume_from(
            strategy_for(&arm.control_variant),
            &ctx,
            policy,
            checkpoint,
            &mut sink,
            DriverConfig {
                surfaces: arm.surfaces.clone(),
                budget_ceiling: std::mem::take(&mut ceiling),
                remaining: std::mem::take(&mut remaining),
                interactive_attendance: interactive,
                task_contract,
                plan_surface_id: is_plan_execute
                    .then(|| hh_control::plan_exec::PLAN_SURFACE_ID.to_string()),
                compute_policy_ref: arm.compute_variant.clone(),
                compute_facts,
                compute_rules,
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
        let timeout_ms = c
            .get("timeout")
            .and_then(Json::as_int)
            .map(|n| n.max(0) as u64);
        // `risk_class` — the `{reversibility, repeat_safety, scope}` object;
        // absent or unparseable leaves `None`, which the ask gate reads as
        // `UNKNOWN` (never-auto, ADR-0031 §2 — fail closed under `bypass`).
        let risk_class = c
            .get("risk_class")
            .and_then(hh_ontology::risk::RiskClass::from_json);
        out.push(HostCap {
            capability_id,
            surface_id,
            requires_approval,
            options,
            timeout_ms,
            risk_class,
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

/// The Stage-1 default `control_strategy` variant (a definition that
/// binds nothing steerable arms the canonical `react/minimal` loop).
pub(crate) const REACT_MINIMAL_VARIANT: &str = "hh/react-minimal";

/// The sealed `control_strategy` slot's bound variant id — read off the
/// root `NativeProcess.slots` the resolver materialised (`resolve` pins
/// `assembly.slots` onto the process; `None` ⇒ the Stage-1 default).
fn control_slot_variant(doc: &hh_hir::document::HirDocument) -> Option<String> {
    for n in &doc.nodes {
        if let KindRecord::AgentProcess(a) = &n.semantic {
            if let AgentProcessBody::Native(np) = &a.body {
                match np.slots.get("control_strategy") {
                    Some(SlotBindings::One(b)) => return Some(b.variant.variant_id.clone()),
                    Some(SlotBindings::Many(v)) => {
                        return v.first().map(|b| b.variant.variant_id.clone())
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

/// The sealed `compute_policy` slot's bound variant spelling (§5e.4) —
/// `None` when the slot is unbound (`static` semantics: the scheduler
/// binds nothing and no `control.compute.*` rows land).
fn compute_slot_variant(doc: &hh_hir::document::HirDocument) -> Option<String> {
    for n in &doc.nodes {
        if let KindRecord::AgentProcess(a) = &n.semantic {
            if let AgentProcessBody::Native(np) = &a.body {
                match np.slots.get("compute_policy") {
                    Some(SlotBindings::One(b)) => return Some(b.variant.variant_id.clone()),
                    Some(SlotBindings::Many(v)) => {
                        return v.first().map(|b| b.variant.variant_id.clone())
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

/// The `ComputeFacts` the sealed `compute_policy` binding declares on its
/// `params` — `parallel_steps`, `subagent_task_targets`,
/// `ensemble{k_max, oracle}`, `verifier`, `profile_capabilities`,
/// `role_table`, `delegation_depth`, `context_label`, `task_value`
/// (§5e.4's ctx members are typed declarations, never prose; anything
/// the params don't carry stays at the fact's zero value and the
/// scheduler reads it as absent).
fn compute_facts_for(doc: &hh_hir::document::HirDocument) -> hh_control::compute::ComputeFacts {
    let mut facts = hh_control::compute::ComputeFacts::default();
    for n in &doc.nodes {
        if let KindRecord::AgentProcess(a) = &n.semantic {
            if let AgentProcessBody::Native(np) = &a.body {
                let params = match np.slots.get("compute_policy") {
                    Some(SlotBindings::One(b)) => Some(&b.params),
                    Some(SlotBindings::Many(v)) => v.first().map(|b| &b.params),
                    _ => None,
                };
                let Some(p) = params else { continue };
                facts.declared_parallel_steps = p
                    .get("parallel_steps")
                    .and_then(Json::as_int)
                    .unwrap_or(0)
                    .max(0) as u32;
                if let Some(Json::Arr(t)) = p.get("subagent_task_targets") {
                    facts.subagent_task_targets = t
                        .iter()
                        .filter_map(Json::as_str)
                        .map(str::to_string)
                        .collect();
                }
                if let Some(e) = p.get("ensemble") {
                    facts.ensemble = Some(hh_control::compute::EnsembleFact {
                        k_max: e.get("k_max").and_then(Json::as_int).unwrap_or(0).max(0) as u32,
                        oracle: e.get("oracle").and_then(Json::as_str).map(str::to_string),
                    });
                }
                facts.verifier = p.get("verifier").and_then(Json::as_str).map(str::to_string);
                if let Some(c) = p.get("profile_capabilities") {
                    facts.profile_capabilities = c.clone();
                }
                if let Some(r) = p.get("role_table") {
                    facts.role_table = r.clone();
                }
                facts.delegation_depth = p
                    .get("delegation_depth")
                    .and_then(Json::as_int)
                    .unwrap_or(0)
                    .max(0) as u32;
                facts.context_label = p
                    .get("context_label")
                    .and_then(Json::as_str)
                    .map(str::to_string);
                facts.task_value = p
                    .get("task_value")
                    .and_then(hh_control::compute::TaskValueFact::from_json);
                // S5.5 prior-cell coordinates + declared prior seeds —
                // `bandit`/`surface_prior` refuse without `profile_ref`
                // (AC-F4-12's unmatched-cell rule).
                facts.profile_ref = p
                    .get("profile_ref")
                    .and_then(Json::as_str)
                    .map(str::to_string);
                facts.snapshot_fingerprint = p
                    .get("snapshot_fingerprint")
                    .and_then(Json::as_str)
                    .map(str::to_string);
                facts.task_class = p
                    .get("task_class")
                    .and_then(Json::as_str)
                    .map(str::to_string);
                if let Some(Json::Arr(seeds)) = p.get("priors") {
                    facts.priors = seeds
                        .iter()
                        .filter_map(hh_control::compute::PriorFact::from_json)
                        .collect();
                }
            }
        }
    }
    facts
}

/// `compute_rules_for(doc)` — the definition-declared `RulesConfig` (the
/// `rules` member of the bound `compute_policy` slot params: conditioned
/// thresholds + `delegation_rules[]` — S5.5's profile-conditioned
/// delegation rules with `debt_ref` per rule). Absent ⇒ `None` (the
/// driver binds the default config).
pub(crate) fn compute_rules_for(
    doc: &hh_hir::document::HirDocument,
) -> Option<hh_control::compute::RulesConfig> {
    for n in &doc.nodes {
        if let KindRecord::AgentProcess(a) = &n.semantic {
            if let AgentProcessBody::Native(np) = &a.body {
                let params = match np.slots.get("compute_policy") {
                    Some(SlotBindings::One(b)) => Some(&b.params),
                    Some(SlotBindings::Many(v)) => v.first().map(|b| &b.params),
                    _ => None,
                };
                if let Some(rc) = params.and_then(|p| p.get("rules")) {
                    return hh_control::compute::RulesConfig::from_json(rc);
                }
            }
        }
    }
    None
}

/// The strategy instance the bound `control_strategy` variant selects —
/// `hh/react-steerable` arms `react/steerable` (R-2.6.1¹); every other
/// `project_task_contract` — the §5f.2 `TaskContract` projection over the
/// sealed document's `Goal` node (S3.10): `success_criteria` + `validates`
/// records with `held_out` visibility read off the criterion node's
/// `ext["visibility"]` member. `None` when the definition carries no
/// `Goal` or the projection refuses (the run then gates on the claim's
/// own divergences only — the T-LCD-03 anchor shape).
pub(crate) fn project_task_contract(
    doc: &hh_hir::document::HirDocument,
) -> Option<hh_verification::gate::TaskContract> {
    doc.nodes.iter().find_map(|n| {
        if let hh_hir::records::KindRecord::Goal(_) = &n.semantic {
            let goal_id = n.version.semantic_id.clone().unwrap_or_default();
            let mut visibility = std::collections::BTreeMap::new();
            for c in &doc.nodes {
                if let (Some(sid), Some(v)) = (
                    c.version.semantic_id.as_ref(),
                    c.ext.get("visibility").and_then(Json::as_str),
                ) {
                    visibility.insert(
                        sid.clone(),
                        if v == "held_out" {
                            hh_verification::vocab::Visibility::HeldOut
                        } else {
                            hh_verification::vocab::Visibility::Visible
                        },
                    );
                }
            }
            hh_verification::gate::project_contract(doc, &goal_id, &visibility).ok()
        } else {
            None
        }
    })
}

/// registered binding resolves to the canonical `react/minimal`
/// interpreter (the family is one loop under presets — ADR-0103 D6).
pub(crate) fn strategy_for(variant_id: &str) -> Box<dyn ControlStrategy> {
    match variant_id.trim_end_matches("@1") {
        "hh/react-steerable" => Box::new(hh_control::react::ReactSteerable::new()),
        "hh/plan-execute" => Box::new(hh_control::plan_exec::PlanExecute::new()),
        _ => Box::new(ReactMinimal::new()),
    }
}

/// `leaf_arm_for(store, run_id)` — the persisted `leaf.arm` record (the
/// durable re-arm pair `runs/<run>/leaf.arm`; absent ⇒ `None` — a run
/// without an arm record never claims a bound control variant).
pub(crate) fn leaf_arm_for(store: &Store, run_id: &str) -> Option<LeafArm> {
    let arm_path = store.root().join("runs").join(run_id).join("leaf.arm");
    std::fs::read_to_string(&arm_path)
        .ok()
        .and_then(|s| hh_wire::json::parse(&s).ok())
        .and_then(|j| LeafArm::from_json(&j))
}

/// The `(steer_mode, concurrent_input)` the bound variant's declared
/// capabilities admit — `capabilities().steering` ⇒ interrupt-at-
/// decision-point under `steer` concurrent input; anything else declines
/// honestly (`Unsupported{by: control_strategy}`, AC-R-2.6.1-10).
pub(crate) fn steering_for(variant_id: &str) -> (SteerMode, ConcurrentInput) {
    if strategy_for(variant_id).capabilities().steering {
        (SteerMode::InterruptAtDecisionPoint, ConcurrentInput::Steer)
    } else {
        (SteerMode::Unsupported, ConcurrentInput::QueueOnly)
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

/// The declared narrowing-leaf ids a manifest records
/// (`manifest.extra["narrowing_leaves"]` — ADR-0168 D3); absent ⇒ `[]`.
/// `respond_permission` folds them into `policy_fingerprint` so a leaf
/// change revokes leases by key construction (ADR-0071 D1).
pub(crate) fn manifest_leaf_ids(manifest: &RunManifest) -> Vec<String> {
    match manifest.extra.get("narrowing_leaves") {
        Some(Json::Arr(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => Vec::new(),
    }
}

/// `environment_spec(environment, workspace_root) → (record, roots,
/// policy, connection_info)` — the **pure** half of environment
/// provisioning: the `EnvironmentRecord`, `Roots` and `ContainmentPolicy`
/// the local binding would run, without touching the store or a driver.
/// `open_new` builds it pre-`open_run` so the manifest can record the
/// binding's enforcement evidence and the `bypass` gate can fail before
/// any run exists (AC-R-2.11.1-7). `provision_environment` consumes the
/// same spec — one construction, never two rules.
fn environment_spec(
    environment: &EnvironmentInput,
    workspace_root: &str,
) -> Result<
    (
        EnvironmentRecord,
        Roots,
        hh_containment::policy::ContainmentPolicy,
        Json,
        Option<hh_containment::backend::Attestation>,
    ),
    EmbedError,
> {
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
            reason: format!("environment class {class} is not served at Stage 2"),
        });
    }
    let ws = workspace_root.to_string();
    let roots_json = match info.get("workspace_roots") {
        Some(Json::Arr(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect::<Vec<_>>(),
        _ => vec![ws.clone()],
    };
    let mut record = EnvironmentRecord {
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
        image_attestation: None,
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
    // S4.14b (R-2.8.4¹) — the binding may *raise* the boundary:
    // `isolation_class` is honoured only above the `process_sandbox`
    // floor (a binding declaring `none` is refused — an external input
    // never loosens the kernel's containment). `substrate_attestation
    // {method, attestation_ref}` carries the measurement the attesting
    // classes (`user_space_kernel`/`microvm`) require at selection;
    // `for_policy` fails closed (`attestation_missing`) without it.
    if let Some(c) = info.get("isolation_class").and_then(Json::as_str) {
        let class = hh_containment::policy::IsolationClass::parse(c).map_err(|e| {
            EmbedError::EnvironmentUnavailable {
                reason: format!("isolation_class:{e}"),
            }
        })?;
        if class == hh_containment::policy::IsolationClass::None {
            return Err(EmbedError::EnvironmentUnavailable {
                reason: "isolation_class_weakening".to_string(),
            });
        }
        policy.proc.isolation_class = class;
    }
    let attestation = match info.get("substrate_attestation") {
        None => None,
        Some(j) => Some(hh_containment::backend::Attestation {
            method: j
                .get("method")
                .and_then(Json::as_str)
                .ok_or_else(|| EmbedError::EnvironmentUnavailable {
                    reason: "substrate_attestation.method".to_string(),
                })?
                .to_string(),
            attestation_ref: j
                .get("attestation_ref")
                .and_then(Json::as_str)
                .ok_or_else(|| EmbedError::EnvironmentUnavailable {
                    reason: "substrate_attestation.attestation_ref".to_string(),
                })?
                .to_string(),
        }),
    };
    policy.compute_ids();
    // S5.8 (R-2.2.5²) — the `substrate_attestation` pair is also the
    // record's `image_attestation` member (one attestation, one spelling
    // — the binding declares it once); record-resolution verifies it
    // against the policy's isolation class before the backend select's
    // `attestation_missing` leg runs.
    record.image_attestation = attestation
        .as_ref()
        .map(|a| hh_env::record::ImageAttestation {
            method: a.method.clone(),
            attestation_ref: a.attestation_ref.clone(),
        });
    record
        .verify_attested(policy.proc.isolation_class)
        .map_err(env_err)?;
    Ok((record, roots, policy, info, attestation))
}

/// The containment `enforcement_evidence` map as a manifest JSON member
/// (`{fs: probed|reported|unknown, …}`) — the gate's verdict, recorded
/// verbatim (ADR-0168 D3: "the manifest records
/// `containment.enforcement_evidence` … from the environment handle").
fn evidence_json(
    ev: &BTreeMap<hh_containment::report::FieldGroup, hh_containment::report::EnforcementEvidence>,
) -> Json {
    let mut m = BTreeMap::new();
    for (g, e) in ev {
        m.insert(g.as_str().to_string(), Json::str(e.as_str()));
    }
    Json::Obj(m)
}

/// `bypass_gate(policy) → Result<(), EmbedError>` — the kernel's half of
/// `BypassWithoutContainment` (ADR-0168 D3): `approval_mode = bypass` is
/// admitted only when the binding's reference backend mints non-`unknown`
/// enforcement evidence for every field group the policy *relies* on —
/// the same pure apply+battery fold the real attach runs
/// (`attach::evidence_preview`), evaluated before `open_run` so the
/// refusal precedes any run's existence.
fn bypass_gate(
    policy: &hh_containment::policy::ContainmentPolicy,
    attestation: Option<hh_containment::backend::Attestation>,
) -> Result<(), EmbedError> {
    let backend = hh_containment::backend::for_policy(policy, attestation).map_err(|e| {
        EmbedError::EnvironmentUnavailable {
            reason: format!("containment_backend:{e:?}"),
        }
    })?;
    let evidence = hh_containment::attach::evidence_preview(&*backend, policy).map_err(|e| {
        EmbedError::EnvironmentUnavailable {
            reason: format!("containment_preview:{e:?}"),
        }
    })?;
    let enforced = hh_containment::attach::relied_groups(policy)
        .iter()
        .all(|g| {
            evidence.get(g) != Some(&hh_containment::report::EnforcementEvidence::Unknown)
                && evidence.contains_key(g)
        });
    if enforced {
        Ok(())
    } else {
        Err(EmbedError::Refused {
            reason: "bypass_without_containment".to_string(),
        })
    }
}

/// The `RealizedSettings` the boundary realizes at Stage 1 — the honest
/// minimal projection (the model role table is the scripted kernel
/// port's; the containment record is the kernel default the environment
/// provisioned under).
pub(crate) fn realized_settings(
    workspace_root: &std::path::Path,
    attendance: &AttendanceDeclaration,
    approval_mode: Option<&str>,
    role_table: Option<&hh_gateway::router::ModelRoleTable>,
) -> RealizedSettings {
    RealizedSettings {
        // The realized `ModelRoleTable` itself when the caller opened it
        // (`open_new`; AC-R-2.3.3-11) — a resume/attach re-projection
        // stays the scripted-port map (the durable record is the run
        // manifest's `profile_binding` member).
        model_role_table_realized: role_table.map(|t| t.to_json()).unwrap_or_else(|| {
            Json::obj([(
                "roles",
                Json::obj([("primary", Json::str("hh-embed/kernel-scripted"))]),
            )])
        }),
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
    match e {
        // S5.8 (R-2.2.5²) — the attesting-class refusal keeps the
        // containment spelling (`attestation_missing`) so a surface sees
        // the same code at record resolution and backend selection.
        hh_env::errors::EnvError::AttestationMissing { isolation_class } => {
            EmbedError::EnvironmentUnavailable {
                reason: format!("attestation_missing:{isolation_class}"),
            }
        }
        other => EmbedError::EnvironmentUnavailable {
            reason: format!("{other:?}"),
        },
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

/// The outcome of `realized_role_table` (AC-R-2.3.3-11; CF-313):
/// `table` is the realized `ModelRoleTable` (its `semantic_id` is the
/// `configuration_id.model_ref`); `primary_identity` is the composition
/// input (`content_hash` when the coordinate resolved, the declared
/// coordinate otherwise); `primary_record_ref` is the resolved record's
/// `version_id` — the only spelling the `open_run` gate admits into
/// `manifest.model_profile_ref` (`None` when the coordinate names no
/// record); `expired_rows` are the `(profile_ref, intent_ref)` pairs the
/// run opens under (the `model.profile.expired_used` rows `open_new`
/// mints and the manifest's `expired_used` member records).
struct RealizedRoles {
    table: hh_gateway::router::ModelRoleTable,
    primary_identity: Option<String>,
    primary_record_ref: Option<String>,
    expired_rows: Vec<(String, String)>,
}

/// The scripted kernel port's route candidate — the one `RouteCandidate`
/// every `RoleBinding` points at (S5.1; AC-R-2.3.3-11): a real `ModelRef`
/// (`profile_ref` is the role's bound coordinate; `provider_model_id` the
/// scripted port's honest identity) over the scripted `ModelCoordinate`.
fn scripted_route_candidate(profile_ref: &str) -> hh_gateway::router::RouteCandidate {
    hh_gateway::router::RouteCandidate {
        model_ref: hh_gateway::plan::ModelRef {
            profile_ref: profile_ref.to_string(),
            provider_model_id: "hh-embed/kernel-scripted".to_string(),
            snapshot_id: None,
            serving_route: None,
            effort: None,
        },
        coordinate: hh_compiler::profile::ModelCoordinate {
            provider_api_family: "hh-embed".to_string(),
            model_family: "kernel-scripted".to_string(),
            model_version: "1".to_string(),
        },
    }
}

/// One `profile_binding` member's value → `(coordinate, intent_ref)` —
/// the value grammar is `set_coordinate`'s (a bare coordinate string or
/// the `{profile_ref: {profile} | "…"}`/`{profile}` spellings —
/// `profile_binding`'s own value shapes; one resolution path, never a
/// second grammar) plus the optional `intent_ref` member an expired
/// binding declares (§5b.3 d.6).
fn profile_binding_entry(j: &Json) -> Option<(String, Option<String>)> {
    let coord = match j {
        Json::Str(v) => v.clone(),
        other => other
            .get("profile_ref")
            .and_then(|r| match r {
                Json::Str(s) => Some(s.clone()),
                Json::Obj(_) => r.get("profile").and_then(Json::as_str).map(str::to_string),
                _ => None,
            })
            .or_else(|| {
                other
                    .get("profile")
                    .and_then(Json::as_str)
                    .map(str::to_string)
            })?,
    };
    let intent = match j {
        Json::Obj(_) => j
            .get("intent_ref")
            .and_then(Json::as_str)
            .map(str::to_string),
        _ => None,
    };
    Some((coord, intent))
}

/// The definition's `primary` profile coordinate — the root native
/// `AgentProcess`'s pinned `native.profile` (CF-313; a selector pin or
/// `unbound` names no coordinate — `profile` is already the bound
/// coordinate from stage-2b's `native_profile`).
fn native_profile_pin(doc: &hh_hir::document::HirDocument) -> Option<String> {
    let root = doc.node(&doc.root.semantic_id)?;
    if let KindRecord::AgentProcess(ap) = &root.semantic {
        if let AgentProcessBody::Native(n) = &ap.body {
            if n.profile.pinned && !n.profile.is_unbound() {
                return Some(n.profile.profile.clone());
            }
        }
    }
    None
}
