//! `adopt_subject*` — the admitted PADR-R5-1 A.2 embed arming/drive owner
//! seam (R5.7; REQ-E-201, REQ-C-103): an **in-process** adoption of a
//! launched subject through the existing runtime. The dossier's
//! non-admission ledger is explicit — `adopt_subject` is an owner
//! interface, never a public RPC; it reuses the existing runtime, one
//! subject and the current fenced writer (`open_resume`'s writer rules —
//! no second run, no copied runtime, no new event class).
//!
//! Validation (AC-R5.7-1): the presented `launch_bound` record is
//! re-read durable through the engine's `launch_binding` reader (R5.6's
//! admitted surface — never caller-trusted), then run / plan / attempt /
//! config / environment / root / generation are checked against the
//! durable prefix. Every refusal lands *before* any subject-run
//! mutation and is typed (`Refused{adopt_*}` — AC-R5.7-2):
//!
//! | check | refusal |
//! |---|---|
//! | presented binding `phase != "bound"` | `adopt_binding_not_bound` |
//! | `subject_writer` absent | `adopt_missing_writer` |
//! | writer not scoped to the subject run | `adopt_writer_run_mismatch` |
//! | subject run unknown | `adopt_unknown_subject` |
//! | `lifecycle.run.finished` present | `adopt_run_finished` |
//! | a live writer session already drives the run | `adopt_duplicate` |
//! | manifest members ≠ the binding's frozen inputs | `adopt_manifest_mismatch:<member>` |
//! | subject's root budget ≠ `subject_root_budget_id` | `adopt_root_mismatch` |
//! | prior declared env ≠ the presented environment | `adopt_environment_mismatch` |
//! | durable `launch_binding` ≠ the presented record | `adopt_stale_binding` |
//! | writer fenced/expired/superseded | `adopt_writer_stale` |
//!
//! Smallest supported composition (recorded in `docs/build/runs/R5.7.md`):
//! `local_host` environment through `provision_environment`, the scripted
//! `EmbedModel` lane (`model_lane = None`), `hh/react-minimal` control +
//! `static` compute, no sealed definition, no router/model_port lane, no
//! supplies/procedures, no host-capability surfaces beyond what `ports`
//! declares, the `context.memory` fold rebuilt from the durable prefix.
//! Absent capabilities stay typed refusals downstream — a launch-created
//! subject manifest carries no `harness_def_ref`, so compiled-plan/task-
//! contract surfaces are simply unarmed, never fabricated.

use std::collections::{BTreeMap, BTreeSet};

use hharness_embed_schema::errors::EmbedError;
use hharness_embed_schema::types::{AttendanceDeclaration, EnvironmentInput};
use hharness_experiment::view::LaunchBinding;
use hharness_ledger::manifest::RunKind;
use hharness_ledger::store::Lease;
use hharness_wire::json::Json;

use crate::open::{
    driver_surfaces, environment_spec, manifest_leaf_ids, realized_settings, sess_manifest_ref,
    steering_for, LeafArm, REACT_MINIMAL_VARIANT,
};
use crate::service::{ledger_err, EmbedService, HostCap, SessionState};

/// `adopt_subject*`'s `binding` argument (PADR-R5-1 A.2): the durable
/// `launch_bound` record the recipe re-presents, the experiment run that
/// owns it, and the subject run's live writer lease.
#[derive(Debug)]
pub struct AdoptBinding<'a> {
    /// The `run_kind = experiment` run carrying the `launch_bound` record.
    pub experiment_run_id: &'a str,
    /// The plan the binding binds (the `launch_binding` reader key —
    /// `LaunchBinding` itself carries no `run_plan_id` member; the plan
    /// is the fold's map key).
    pub run_plan_id: &'a str,
    /// The `launch_binding(run_plan_id)` record the caller presents —
    /// re-read durable inside `adopt_subject`; a stale, not-bound or
    /// member-mismatched presentation refuses before any mutation.
    pub binding: &'a LaunchBinding,
    /// The subject run's live writer lease (the launch's
    /// `subject_writer`) — `None` refuses `adopt_missing_writer`; a
    /// fenced, expired or superseded lease refuses `adopt_writer_stale`.
    pub subject_writer: Option<&'a Lease>,
}

/// `adopt_subject*`'s `ports` argument — the host-capability declarations
/// whose surfaces the adopted driver arms (the same shape `open_session`'s
/// `supplies.host_capabilities` realizes through `driver_surfaces`).
#[derive(Debug)]
pub struct AdoptPorts<'a> {
    /// The declared host capabilities the adopted session arms.
    pub host_caps: &'a [HostCap],
}

/// The adoption product — the armed session plus the binding/coordinate
/// joins the recipe records.
#[derive(Debug, Clone)]
pub struct AdoptOutcome {
    /// The live session id (the boundary's `session_id` handle).
    pub session_id: String,
    /// The adopted subject run.
    pub subject_run_id: String,
    /// The durable intent this adoption binds (`launch_bound.intent_ref`).
    pub intent_ref: String,
    /// The binding's plan.
    pub run_plan_id: String,
    /// The bound attempt ordinal.
    pub attempt_no: u32,
    /// The subject writer's generation — the launch's own fenced writer,
    /// renewed; never a second writer (AC-2: no second run/copied runtime).
    pub writer_generation: u64,
    /// `armed` (fresh leaf) | `resumed` (persisted `leaf.arm` + checkpoint).
    pub armed: String,
    /// The adopted environment handle, when this adoption provisioned one
    /// (a resumed adoption reuses the durable environment record instead —
    /// `None`, matching `open_resume`'s durable-resume posture).
    pub env_handle_id: Option<String>,
    /// The durable cursor the adoption observed (post-arm head seq).
    pub head_seq: i64,
}

fn refused(reason: impl Into<String>) -> EmbedError {
    EmbedError::Refused {
        reason: reason.into(),
    }
}

/// `Refused{adopt_manifest_mismatch:<member>}` — one subject-run manifest
/// member conflicts with the binding's frozen inputs or record.
fn manifest_mismatch(member: &str) -> EmbedError {
    refused(format!("adopt_manifest_mismatch:{member}"))
}

fn str_member<'a>(j: &'a Json, k: &str) -> Option<&'a str> {
    j.get(k).and_then(Json::as_str)
}

impl EmbedService {
    /// `adopt_subject(binding, environment, ports)*` — adopt a launched
    /// subject through the existing runtime (PADR-R5-1 A.2 owner seam;
    /// in-process, never an RPC). See the module doc for the validation
    /// table and the refused set.
    pub fn adopt_subject(
        &mut self,
        claim: AdoptBinding<'_>,
        environment: &EnvironmentInput,
        ports: &AdoptPorts<'_>,
    ) -> Result<AdoptOutcome, EmbedError> {
        let b = claim.binding;
        let subject_run_id = b.subject_run_id.clone();
        let run_plan_id = claim.run_plan_id.to_string();
        // ── presented-claim checks (pure) ─────────────────────────────
        if b.phase != "bound" {
            // An `intent`/`aborted` record is not a usable binding —
            // funding/root are unbound until publish (R5.6 reader contract).
            return Err(refused("adopt_binding_not_bound"));
        }
        let presented_writer = claim
            .subject_writer
            .ok_or_else(|| refused("adopt_missing_writer"))?;
        if presented_writer.run_id != subject_run_id {
            return Err(refused("adopt_writer_run_mismatch"));
        }
        // ── subject-run existence / finished / duplicate (pure) ───────
        if !self.store.has_run(&subject_run_id) {
            return Err(refused("adopt_unknown_subject"));
        }
        // One read of the durable prefix feeds every subject-run check.
        let (finished, declared_env, subject_root_id, bound_row, active_turn, finished_turns) = {
            let events = self.store.events(&subject_run_id).map_err(ledger_err)?;
            let mut finished = false;
            let mut declared_env: Option<(String, Json)> = None;
            let mut subject_root_id: Option<String> = None;
            let mut bound_row: Option<Json> = None;
            let mut active = String::new();
            let mut finished_turns = BTreeSet::new();
            for e in events {
                match e.class.as_str() {
                    "lifecycle.run.finished" => finished = true,
                    // The subject's own launch-binding proof — the
                    // `measurement.experiment.bound` row the launch
                    // stamped on this stream carries the frozen
                    // plan/attempt/config coordinates adoption checks.
                    "measurement.experiment.bound" => bound_row = Some(e.payload.clone()),
                    // The env the run already carries (`env.open`'s
                    // second-environment rule means the *latest*
                    // declared row is the one a re-adoption must match).
                    "action.environment.declared" => {
                        declared_env = Some((
                            str_member(&e.payload, "env_handle")
                                .unwrap_or_default()
                                .to_string(),
                            e.payload.clone(),
                        ));
                    }
                    // The subject's own root budget — the parentless
                    // `control.budget.allocated` row (the engine's
                    // `subject_root` fold, same durable fact).
                    "control.budget.allocated" if e.payload.get("parent").is_none() => {
                        subject_root_id = Some(
                            str_member(&e.payload, "budget_id")
                                .unwrap_or_default()
                                .to_string(),
                        );
                    }
                    "lifecycle.turn.started" => {
                        active = str_member(&e.payload, "turn_id")
                            .unwrap_or_default()
                            .to_string();
                    }
                    "lifecycle.turn.finished" => {
                        if let Some(t) = str_member(&e.payload, "turn_id").map(str::to_string) {
                            finished_turns.insert(t);
                        }
                    }
                    _ => {}
                }
            }
            (
                finished,
                declared_env,
                subject_root_id,
                bound_row,
                active,
                finished_turns,
            )
        };
        if finished {
            return Err(refused("adopt_run_finished"));
        }
        if self
            .sessions
            .values()
            .any(|s| s.run_id == subject_run_id && !s.attach && s.detached.is_none() && !s.finished)
        {
            return Err(refused("adopt_duplicate"));
        }
        // ── manifest ↔ binding (run/plan/attempt/config/slice) ────────
        // The durable join keys: the launch-stamped `bound` row carries
        // the frozen coordinates (the `launch_bound` record's own frozen
        // `inputs` live only on the intent-phase row — the bound record
        // is the subject-stream proof adoption checks).
        let manifest = self
            .store
            .manifest(&subject_run_id)
            .map_err(ledger_err)?
            .clone();
        if manifest.run_kind != RunKind::Agent {
            return Err(manifest_mismatch("run_kind"));
        }
        let bound = bound_row.ok_or_else(|| manifest_mismatch("bound"))?;
        let exp = manifest
            .experiment
            .as_ref()
            .ok_or_else(|| manifest_mismatch("experiment"))?;
        if exp.experiment_run_id.as_deref() != Some(claim.experiment_run_id) {
            return Err(manifest_mismatch("experiment_run_id"));
        }
        let attempt_ok = bound
            .get("attempt_no")
            .and_then(Json::as_int)
            .map(|v| v as u64)
            == Some(b.attempt_no as u64);
        if !attempt_ok || exp.attempt_no != Some(b.attempt_no as u64) {
            return Err(manifest_mismatch("attempt_no"));
        }
        if exp.cell_id.as_deref() != str_member(&bound, "cell_id") {
            return Err(manifest_mismatch("cell_id"));
        }
        if exp.arm_id.as_deref() != str_member(&bound, "arm_id") {
            return Err(manifest_mismatch("arm_id"));
        }
        if exp.replicate_index
            != bound
                .get("replicate_index")
                .and_then(Json::as_int)
                .map(|v| v.max(0) as u64)
        {
            return Err(manifest_mismatch("replicate_index"));
        }
        if manifest.configuration_id.as_deref() != str_member(&bound, "configuration_id") {
            return Err(manifest_mismatch("configuration_id"));
        }
        if manifest.configuration_version_id.as_deref()
            != str_member(&bound, "configuration_version_id")
        {
            return Err(manifest_mismatch("configuration_version_id"));
        }
        if manifest.budget.as_deref() != b.funding_slice_ref.as_deref()
            || str_member(&bound, "budget_id") != b.funding_slice_ref.as_deref()
        {
            return Err(manifest_mismatch("funding_slice"));
        }
        // ── root (the subject's own budget root vs the record) ────────
        if subject_root_id.as_deref() != b.subject_root_budget_id.as_deref() {
            return Err(refused("adopt_root_mismatch"));
        }
        // ── environment (a prior declared env must re-derive equal) ───
        let ws = self.workspace_root().display().to_string();
        let (record, _, _, _, _) = environment_spec(environment, &ws)?;
        if let Some((_, declared)) = &declared_env {
            // The durable `image` is the *resolved* image (`content_address`
            // member) — the record's `ImageRef` names the same address under
            // `id`. Same class + same resolved image = same environment.
            let same = str_member(declared, "class") == Some(record.class.as_str())
                && declared
                    .get("image")
                    .and_then(|i| str_member(i, "content_address"))
                    == record.image.to_json().get("id").and_then(|i| i.as_str());
            if !same {
                return Err(refused("adopt_environment_mismatch"));
            }
        }
        // ── durable binding re-read (the admitted R5.6 reader) ────────
        // `launch_binding(run_plan_id)` folds `ExperimentView` — the same
        // projection applied here read-only, so the validation never takes
        // the experiment run's writer (a recipe's live engine holder is
        // never fenced for a read).
        let durable = {
            let view = hharness_experiment::view::ExperimentView::fold(
                self.store
                    .events(claim.experiment_run_id)
                    .map_err(ledger_err)?,
            );
            view.plans
                .get(&run_plan_id)
                .and_then(|ps| ps.launch_binding.clone())
        };
        if durable.as_ref() != Some(b) {
            return Err(refused("adopt_stale_binding"));
        }
        // ── generation: the presented writer must be the live one ─────
        // `renew` proves lease_id + generation + unexpired against the
        // durable record and extends the TTL — a stale, superseded or
        // expired writer fails `Fenced` with no subject-run mutation.
        let lease = self.store.renew(presented_writer).map_err(|e| match e {
            hharness_ledger::errors::LedgerError::Fenced {
                current_generation, ..
            } => refused(format!(
                "adopt_writer_stale: fenced (current generation {current_generation})"
            )),
            other => ledger_err(other),
        })?;
        // ── arm / resume through the existing runtime (durable-first) ─
        // Environment: a fresh subject provisions `local_host` through
        // the one `provision_environment` path; a run that already
        // carries a matching `declared` env reuses the durable record —
        // re-adoption never mints a second environment.
        let (env_handle_id, env_json) = match &declared_env {
            Some((handle, _)) => (Some(handle.clone()), Json::Null),
            None => self.provision_environment(&subject_run_id, &lease, environment)?,
        };
        let session_id = self.alloc("sess");
        // The adoption fact — `lifecycle.session.attached` extended with
        // the binding join (dossier A.3: adopt facts extend existing
        // lifecycle payloads; no new event class).
        self.mint(
            &subject_run_id,
            &lease,
            "lifecycle.session.attached",
            Json::obj([
                ("session_id", Json::str(session_id.clone())),
                ("mode", Json::str("adopt")),
                ("attachment_id", Json::str(session_id.clone())),
                ("intent_ref", Json::str(b.intent_ref.clone())),
                ("run_plan_id", Json::str(run_plan_id.clone())),
                ("attempt_no", Json::Int(b.attempt_no as i64)),
            ]),
        )?;
        // Arm or resume: the persisted `leaf.arm` + `leaf.checkpoint`
        // pair is the durable resume marker (S2.3 — `open_resume`'s
        // checkpoint rule); absent ⇒ the fresh `arm_driver` path.
        let run_dir = self.store.root().join("runs").join(&subject_run_id);
        let ckpt_path = run_dir.join("leaf.checkpoint");
        let arm_path = run_dir.join("leaf.arm");
        let resume_ok = ckpt_path.exists() && arm_path.exists();
        let (driver, leaf_arm, model_fail_plan, armed) = if resume_ok {
            let bytes = std::fs::read(&ckpt_path).map_err(|_| EmbedError::Refused {
                reason: "resume_checkpoint_unavailable".to_string(),
            })?;
            let (d, a, f) = self.arm_resume_driver(&subject_run_id, &lease, &bytes, &manifest)?;
            (d, a, f, "resumed".to_string())
        } else {
            let surfaces = driver_surfaces(ports.host_caps);
            let (d, sp) = self.arm_driver(
                &subject_run_id,
                &lease,
                &surfaces,
                &manifest,
                None,
                REACT_MINIMAL_VARIANT,
                "static",
                None,
            )?;
            (
                d,
                LeafArm {
                    surfaces,
                    budget_ceiling: BTreeMap::new(),
                    remaining: BTreeMap::new(),
                    control_variant: REACT_MINIMAL_VARIANT.to_string(),
                    compute_variant: "static".to_string(),
                    // R6.38 — the arm's resolved params persist for the
                    // same resume parity the `new` path records.
                    strategy_params: Some(hharness_control::wire::strategy_params_to_json(&sp)),
                },
                std::collections::VecDeque::new(),
                "armed".to_string(),
            )
        };
        // The shared context/memory fold — rebuilt from the durable
        // prefix (the resume path's `KernelContext::fold`; a fresh
        // subject folds empty, never a fabricated history).
        let mem_ctx = crate::runtime::KernelContext::fold(
            &subject_run_id,
            self.store.events(&subject_run_id).map_err(ledger_err)?,
            &self.store,
            self.store.now_ms(),
        );
        let manifest_attendance = AttendanceDeclaration {
            value: manifest.attendance.0.as_str().to_string(),
            source: manifest.attendance.1.as_str().to_string(),
        };
        let realized = realized_settings(
            self.workspace_root(),
            &manifest_attendance,
            manifest.extra.get("approval_mode").and_then(Json::as_str),
            None,
        );
        let head = self.store.head(&subject_run_id).map_err(ledger_err)?;
        let sess = SessionState {
            run_id: subject_run_id.clone(),
            attach: false,
            lease: Some(lease.clone()),
            manifest_ref: sess_manifest_ref(&manifest),
            realized,
            driver: Some(driver),
            steering: steering_for(&self.registry, &leaf_arm.control_variant),
            mem_ctx: Some(std::rc::Rc::new(std::cell::RefCell::new(mem_ctx))),
            env_json,
            env_handle_id: env_handle_id.clone(),
            host_caps: ports.host_caps.to_vec(),
            turn_active: !active_turn.is_empty() && !finished_turns.contains(&active_turn),
            active_turn: if active_turn.is_empty() {
                "turn-1".to_string()
            } else {
                active_turn
            },
            finished: false,
            detached: None,
            authority_caps: manifest
                .harness_def_ref
                .as_deref()
                .and_then(|r| self.persisted_definition(r).ok())
                .map(|d| hharness_monitor::mint::cap_rows(&d.document))
                .unwrap_or_default(),
            narrowing_leaf_ids: manifest_leaf_ids(&manifest),
            pendings: BTreeMap::new(),
            decided: BTreeMap::new(),
            delivered_wokens: BTreeSet::new(),
            host_asks: BTreeMap::new(),
            idem: BTreeMap::new(),
            budget_ceiling: BTreeMap::new(),
            leaf_arm,
            scan_seq: head.seq,
            next_invoke: None,
            next_completion: String::new(),
            next_response_ref: String::new(),
            model_lane: None,
            model_surface: None,
            model_fail_plan,
            client: None,
            contract_json: None,
            sink_seq: head.seq as i64,
            exec_caps: Vec::new(),
        };
        self.sessions.insert(session_id.clone(), sess);
        // Persist the resume-by-leaf pair — a later adoption resumes the
        // leaf exactly where this arm left it (S2.3).
        self.persist_leaf(&session_id);
        Ok(AdoptOutcome {
            session_id,
            subject_run_id,
            intent_ref: b.intent_ref.clone(),
            run_plan_id,
            attempt_no: b.attempt_no,
            writer_generation: lease.generation,
            armed,
            env_handle_id,
            head_seq: head.seq as i64,
        })
    }
}
