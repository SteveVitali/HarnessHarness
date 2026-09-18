//! Group W (work) and Group R (read) operation handlers — the dispatch
//! targets `service.rs` routes after the hello/capability/experimental
//! gates. Every writer op goes through `writer_session` (attach sessions
//! are refused `read_only` — I6); every mutation mints under the
//! session's fenced writer lease.

use crate::frames::FrameAdapter;
use crate::inject;
use crate::open::{attendance_async, realized_settings, sess_manifest_ref, session_json};
use crate::service::{ledger_err, EmbedService, SubState};
use hh_control::vocab::{Cue, HumanInput};
use hh_embed_schema::errors::EmbedError;
use hh_embed_schema::strict::StrictObj;
use hh_embed_schema::types::*;
use hh_ledger::event::{Cursor, Direction as ReadDir};
use hh_ledger::manifest::{LineageLink, RunKind, RunManifest};
use hh_ledger::views::ViewKind;
use hh_wire::json::Json;

/// The read page bound when `limit = 0` (server default).
const DEFAULT_PAGE: usize = 200;

impl EmbedService {
    // ── Group W ───────────────────────────────────────────────────────

    /// `submit` — stage the scripted model call from the input blocks,
    /// wake the loop (`follow_up` cue), and drive until it parks or the
    /// `hh.submit` stop-rule fires (`Accepted{turn_id}`).
    pub(crate) fn submit(&mut self, params: &Json) -> Result<Json, EmbedError> {
        inject::refuse_handle_keys(params, "submit")?;
        inject::refuse_secrets(params)?;
        let p = SubmitParams::from_json(params)?;
        {
            let s = self.writer_session(&p.session_id)?;
            if let Some(hit) = s.idem.get(&p.idempotency_key) {
                return Ok(hit.clone());
            }
            if s.finished {
                return Err(EmbedError::Draining);
            }
            if s.turn_active {
                return Err(EmbedError::TurnActive);
            }
        }
        // The scripted model port reads the staged input: one
        // `{kind:"invoke"}` block routes to that capability surface;
        // otherwise the input is the completion the `hh.submit` call
        // carries (its first `text` member, else the canonical input).
        let mut invoke: Option<(String, Json)> = None;
        let mut completion = String::new();
        for block in &p.input {
            match block.get("kind").and_then(Json::as_str) {
                Some("invoke") => {
                    let cap = block
                        .get("capability")
                        .or_else(|| block.get("surface"))
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string();
                    if !cap.is_empty() && invoke.is_none() {
                        invoke = Some((cap, block.get("args").cloned().unwrap_or(Json::Null)));
                    }
                }
                _ => {
                    if completion.is_empty() {
                        if let Some(t) = block.get("text").and_then(Json::as_str) {
                            completion = t.to_string();
                        }
                    }
                }
            }
        }
        if completion.is_empty() && invoke.is_none() {
            completion = Json::Arr(p.input.clone()).to_canonical_string();
        }
        let response_ref = self.alloc("resp");
        {
            let s = self.session_mut(&p.session_id)?;
            s.next_invoke = invoke;
            s.next_completion = completion.clone();
            s.next_response_ref = response_ref;
        }
        let payload_ref = self.record_input(&p.session_id, &p.input)?;
        {
            let s = self.session_mut(&p.session_id)?;
            if let Some(d) = s.driver.as_mut() {
                d.submit(Cue::HumanInput(HumanInput::FollowUp { payload_ref }));
            }
        }
        self.drive(&p.session_id)?;
        let turn = self.session(&p.session_id)?.active_turn.clone();
        let out = Accepted { turn_id: turn }.to_json();
        self.session_mut(&p.session_id)?
            .idem
            .insert(p.idempotency_key, out.clone());
        Ok(out)
    }

    /// Mint the submitted input as `context.artefact.delivered` durable
    /// rows (§8394: `submit.input` items are `ContextItem`/`Procedure`
    /// deliveries — the class table's registered row; one row per input
    /// block, S1.26); returns the artefact id the follow-up cue carries
    /// (the cue names a record, never bytes, I2). Payload shape per
    /// §5c.1: `{artefact_id, delivery_id, kind, rendering_ref?,
    /// by_reference}` — the principal's inline input is
    /// `kind:"instruction"`, delivered by value; an `{kind:"invoke"}`
    /// block is a capability delivery (`kind:"capability"`).
    fn record_input(&mut self, sess_id: &str, input: &[Json]) -> Result<String, EmbedError> {
        let (run_id, lease) = {
            let s = self.session(sess_id)?;
            (
                s.run_id.clone(),
                s.lease.clone().ok_or(EmbedError::Refused {
                    reason: "session_is_read_only".to_string(),
                })?,
            )
        };
        let mut first = String::new();
        for (i, block) in input.iter().enumerate() {
            let input_id = self.alloc("input");
            if i == 0 {
                first = input_id.clone();
            }
            let kind = match block.get("kind").and_then(Json::as_str) {
                Some("invoke") => "capability",
                _ => "instruction",
            };
            self.mint(
                &run_id,
                &lease,
                "context.artefact.delivered",
                Json::obj([
                    ("artefact_id", Json::str(input_id.clone())),
                    ("delivery_id", Json::str(input_id.clone())),
                    ("kind", Json::str(kind)),
                    ("by_reference", Json::Bool(false)),
                ]),
            )?;
        }
        Ok(first)
    }

    /// `cancel` — the principal's `interrupt` cue. `scope{kind:"turn"}`
    /// names the active turn (`TurnMismatch` otherwise); `scope{kind:
    /// "run"}` interrupts whatever is live.
    pub(crate) fn cancel(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = CancelParams::from_json(params)?;
        {
            let s = self.writer_session(&p.session_id)?;
            if let Some(k) = &p.idempotency_key {
                if let Some(hit) = s.idem.get(k) {
                    return Ok(hit.clone());
                }
            }
            if let CancelScope::Turn { turn_id } = &p.scope {
                if turn_id != &s.active_turn {
                    return Err(EmbedError::TurnMismatch {
                        active_turn_id: s.active_turn.clone(),
                    });
                }
            }
        }
        {
            let s = self.session_mut(&p.session_id)?;
            if let Some(d) = s.driver.as_mut() {
                d.submit(Cue::HumanInput(HumanInput::Interrupt));
            }
        }
        let _ = self.drive(&p.session_id);
        let out = acknowledged_json();
        if let Some(k) = p.idempotency_key {
            self.session_mut(&p.session_id)?.idem.insert(k, out.clone());
        }
        Ok(out)
    }

    /// `steer` — the react/minimal boundary declares
    /// `steer_mode = unsupported`; a correctly addressed steer is
    /// `TurnMismatch`-checked then refused `Unsupported` (the honest
    /// answer — the strategy surface is closed, not silently queued).
    pub(crate) fn steer(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = SteerParams::from_json(params)?;
        let s = self.writer_session(&p.session_id)?;
        if let Some(t) = &p.expected_turn_id {
            if t != &s.active_turn {
                return Err(EmbedError::TurnMismatch {
                    active_turn_id: s.active_turn.clone(),
                });
            }
        }
        Err(EmbedError::Unsupported {
            by: "control_strategy".to_string(),
        })
    }

    /// `respond_permission` — the host's answer to a live
    /// `security.permission.pending`: `selected{option_id}` must name an
    /// offered option (`OptionNotOffered`); the decision mints
    /// `security.permission.decided` and resumes the loop when the ask
    /// covered an effect (`approval` cue). `Recorded`.
    pub(crate) fn respond_permission(&mut self, params: &Json) -> Result<Json, EmbedError> {
        inject::refuse_secrets(params)?;
        let p = RespondPermissionParams::from_json(params)?;
        {
            let s = self.writer_session(&p.session_id)?;
            if let Some(hit) = s.idem.get(&p.idempotency_key) {
                return Ok(hit.clone());
            }
            if s.decided.contains_key(&p.permission_id) {
                return Err(EmbedError::AlreadyDecided {
                    permission_id: p.permission_id.clone(),
                });
            }
            let pending =
                s.pendings
                    .get(&p.permission_id)
                    .ok_or(EmbedError::UnknownPermission {
                        permission_id: p.permission_id.clone(),
                    })?;
            if let PermissionOutcome::Selected { option_id } = &p.outcome {
                if !pending.options.is_empty() && !pending.options.iter().any(|o| o == option_id) {
                    return Err(EmbedError::OptionNotOffered {
                        option_id: option_id.clone(),
                    });
                }
            }
        }
        let (run_id, lease, effect_id, allow) = {
            let s = self.session(&p.session_id)?;
            let pending = s.pendings.get(&p.permission_id).unwrap();
            let allow = matches!(
                p.outcome,
                PermissionOutcome::Selected { ref option_id } if option_id.starts_with("allow")
            );
            (
                s.run_id.clone(),
                s.lease.clone().ok_or(EmbedError::Refused {
                    reason: "session_is_read_only".to_string(),
                })?,
                pending.effect_id.clone(),
                allow,
            )
        };
        let decision = match &p.outcome {
            PermissionOutcome::Selected { option_id } => Json::obj([
                ("kind", Json::str("selected")),
                ("option_id", Json::str(option_id.clone())),
            ]),
            PermissionOutcome::Cancelled => Json::obj([("kind", Json::str("cancelled"))]),
        };
        // `security.permission.decided` is audit-grade — the payload must
        // carry the class table's declared fields: the ask's `proposal`,
        // the chosen option id (`decision`) and the deciding party
        // (`decider`); `outcome` is not a declared member.
        let (decision_name, proposal) = {
            let s = self.session(&p.session_id)?;
            let pending = s.pendings.get(&p.permission_id);
            (
                match &p.outcome {
                    PermissionOutcome::Selected { option_id } => option_id.clone(),
                    PermissionOutcome::Cancelled => "cancelled".to_string(),
                },
                pending.map(|pa| pa.proposal.clone()).unwrap_or_default(),
            )
        };
        self.mint(
            &run_id,
            &lease,
            "security.permission.decided",
            Json::obj([
                ("permission_id", Json::str(p.permission_id.clone())),
                ("proposal", Json::str(proposal)),
                ("decision", Json::str(decision_name)),
                ("decider", Json::str("principal")),
            ]),
        )?;
        {
            let s = self.session_mut(&p.session_id)?;
            s.decided.insert(p.permission_id.clone(), decision);
            s.pendings.remove(&p.permission_id);
            if let Some(ef) = &effect_id {
                if let Some(d) = s.driver.as_mut() {
                    d.submit(Cue::HumanInput(HumanInput::Approval {
                        effect_id: ef.clone(),
                        allow,
                    }));
                }
            }
        }
        if effect_id.is_some() {
            let _ = self.drive(&p.session_id);
        }
        let out = recorded_json(&p.permission_id);
        self.session_mut(&p.session_id)?
            .idem
            .insert(p.idempotency_key, out.clone());
        Ok(out)
    }

    /// `amend` — the ADR-0216 OQ-468 interim op. At Stage 1 the one
    /// honoured target is `budget` (the attended-exhaustion wake):
    /// `value = {dimensions: {<dim>: <new_ceiling>}}` mints a
    /// `control.budget.amended` audit row per dimension, re-arms the
    /// driver's hard ceiling (`amend_budget_ceiling`), and wakes the
    /// parked escalation (`follow_up` → the loop re-proposes under the
    /// new `remaining`). I-1 applies: a *loosening* amendment requires
    /// the run's `interactive` attendance or a caller `attestation`;
    /// anything less is `AuthorityWideningRequiresHuman`. A tightening
    /// amendment is admitted unconditionally. `amend{approval_mode |
    /// attendance}` answers `Refused{stage_pending}` (C1/Stage 2 —
    /// ADR-0168(e)).
    pub(crate) fn amend(&mut self, params: &Json) -> Result<Json, EmbedError> {
        inject::refuse_handle_keys(params, "amend")?;
        inject::refuse_secrets(params)?;
        let p = AmendParams::from_json(params)?;
        {
            let s = self.writer_session(&p.session_id)?;
            if let Some(k) = &p.idempotency_key {
                if let Some(hit) = s.idem.get(k) {
                    return Ok(hit.clone());
                }
            }
            if s.finished {
                return Err(EmbedError::Draining);
            }
        }
        if p.target != "budget" {
            return Err(EmbedError::Refused {
                reason: "stage_pending".to_string(),
            });
        }
        let dims: Vec<(String, i64)> = match p.value.get("dimensions") {
            Some(Json::Obj(m)) => m
                .iter()
                .filter_map(|(d, v)| v.as_int().map(|n| (d.clone(), n)))
                .collect(),
            _ => match (
                p.value.get("dimension").and_then(Json::as_str),
                p.value.get("ceiling").and_then(Json::as_int),
            ) {
                (Some(d), Some(n)) => vec![(d.to_string(), n)],
                _ => {
                    return Err(EmbedError::SchemaViolation {
                        path: "amend/value".to_string(),
                        code: "bad_amend_value".to_string(),
                    })
                }
            },
        };
        if dims.is_empty() {
            return Err(EmbedError::SchemaViolation {
                path: "amend/value".to_string(),
                code: "bad_amend_value".to_string(),
            });
        }
        let (run_id, lease, manifest, realized) = {
            let s = self.session(&p.session_id)?;
            (
                s.run_id.clone(),
                s.lease.clone().ok_or(EmbedError::Refused {
                    reason: "session_is_read_only".to_string(),
                })?,
                self.store.manifest(&s.run_id).map_err(ledger_err)?.clone(),
                s.realized.clone(),
            )
        };
        let interactive =
            manifest.attendance.0 == hh_ledger::manifest::AttendanceValue::Interactive;
        let budget_id = manifest.budget.clone().unwrap_or_else(|| "b-1".to_string());
        for (dim, new_cap) in &dims {
            // I-1 — a loosening amendment widens spend: human at the
            // terminal (interactive manifest attendance) or a caller
            // attestation; tightening is admitted unconditionally.
            let old_cap = {
                let s = self.session(&p.session_id)?;
                s.budget_ceiling.get(dim).copied()
            };
            let loosening = old_cap.map(|o| *new_cap > o).unwrap_or(true);
            if loosening && !interactive && p.attestation.is_none() {
                return Err(EmbedError::AuthorityWideningRequiresHuman {
                    detail: format!(
                        "amend(budget) loosens dimension {dim} ({old_cap:?} → {new_cap})                          without interactive attendance or attestation"
                    ),
                });
            }
            self.mint(
                &run_id,
                &lease,
                "control.budget.amended",
                Json::obj([
                    ("budget_id", Json::str(budget_id.clone())),
                    ("dimension", Json::str(dim.clone())),
                    ("value", Json::Int(*new_cap)),
                    ("limit", Json::Int(*new_cap)),
                    (
                        "old",
                        Json::str(
                            Json::obj([("hard", old_cap.map(Json::Int).unwrap_or(Json::Null))])
                                .to_canonical_string(),
                        ),
                    ),
                    (
                        "new",
                        Json::str(Json::obj([("hard", Json::Int(*new_cap))]).to_canonical_string()),
                    ),
                    ("authority", Json::str("principal")),
                ]),
            )?;
            {
                let s = self.session_mut(&p.session_id)?;
                s.budget_ceiling.insert(dim.clone(), *new_cap);
                if let Some(d) = s.driver.as_mut() {
                    d.amend_budget_ceiling(dim, *new_cap);
                }
            }
        }
        // `lifecycle.surface.invoked` — an amendment is a run-amending
        // surface invocation.
        if let Some(inv) = &p.invocation {
            self.mint(&run_id, &lease, "lifecycle.surface.invoked", inv.to_json())?;
        }
        // Wake the parked loop — the escalation decision parked the
        // drive on `human_input`; the amendment is the principal's
        // follow-up (the durable `control.budget.amended` row is the
        // record; the cue carries its artefact ref).
        {
            let amend_ref = self.alloc("amend");
            let s = self.session_mut(&p.session_id)?;
            if let Some(d) = s.driver.as_mut() {
                d.submit(Cue::HumanInput(HumanInput::FollowUp {
                    payload_ref: amend_ref,
                }));
            }
        }
        self.drive(&p.session_id)?;
        let head = self.store.head(&run_id).map_err(ledger_err)?;
        let out = session_json(
            &p.session_id,
            &run_id,
            &p.session_id,
            &sess_manifest_ref(&manifest),
            &manifest.configuration_id.clone().unwrap_or_default(),
            &manifest
                .configuration_version_id
                .clone()
                .unwrap_or_default(),
            &realized,
            head.seq as i64,
        );
        if let Some(k) = &p.idempotency_key {
            self.session_mut(&p.session_id)?
                .idem
                .insert(k.clone(), out.clone());
        }
        Ok(out)
    }

    /// `fork` — `open_run` a child bound to the parent's head at `at`
    /// (`forked_from{run_id, at_seq, head_hash}` — the ledger re-checks
    /// the anchor; `manifest_delta` applies no Stage-1 fields and is
    /// refused non-empty rather than silently dropped). The child gets
    /// a fresh writer lease + session.
    pub(crate) fn fork(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = ForkParams::from_json(params)?;
        if p.manifest_delta.is_some() {
            return Err(EmbedError::Refused {
                reason: "manifest_delta_unsupported".to_string(),
            });
        }
        let (run_id, at_seq, head_hash) = {
            let s = self.writer_session(&p.session_id)?;
            let events = self.store.events(&s.run_id).map_err(ledger_err)?;
            let at_seq = match &p.at {
                ForkPoint::Seq(n) => *n as u64,
                ForkPoint::EventRef { event_id, .. } => events
                    .iter()
                    .find(|e| &e.event_id == event_id)
                    .map(|e| e.seq)
                    .ok_or_else(|| EmbedError::SchemaViolation {
                        path: "fork/at/event_id".to_string(),
                        code: "unknown_event".to_string(),
                    })?,
            };
            let head = events.iter().find(|e| e.seq == at_seq).ok_or_else(|| {
                EmbedError::SchemaViolation {
                    path: "fork/at".to_string(),
                    code: "seq_out_of_range".to_string(),
                }
            })?;
            (s.run_id.clone(), at_seq, head.hash.clone())
        };
        let parent = self.store.manifest(&run_id).map_err(ledger_err)?.clone();
        let mut child = RunManifest::minimal(RunKind::Agent);
        child.configuration_id = parent.configuration_id.clone();
        child.configuration_version_id = parent.configuration_version_id.clone();
        child.harness_def_ref = parent.harness_def_ref.clone();
        child.attendance = parent.attendance;
        child.budget = parent.budget.clone();
        child.forked_from = Some(LineageLink {
            run_id: run_id.clone(),
            at_seq,
            head_hash,
        });
        child.parent_run_id = Some(run_id.clone());
        let holder = self.holder.clone();
        let (child_run, lease) = self
            .store
            .open_run(child.clone(), &holder)
            .map_err(ledger_err)?;
        // `lifecycle.surface.invoked` — a fork opens a child run.
        if let Some(inv) = &p.invocation {
            self.mint(
                &child_run,
                &lease,
                "lifecycle.surface.invoked",
                inv.to_json(),
            )?;
        }
        let session_id = self.alloc("sess");
        self.mint(
            &child_run,
            &lease,
            "lifecycle.session.attached",
            Json::obj([
                ("session_id", Json::str(session_id.clone())),
                ("mode", Json::str("fork")),
                ("attachment_id", Json::str(session_id.clone())),
            ]),
        )?;
        let realized = realized_settings(&self.workspace_root, &attendance_async(), None);
        let head = self.store.head(&child_run).map_err(ledger_err)?;
        let sess = crate::service::SessionState {
            run_id: child_run.clone(),
            attach: false,
            lease: Some(lease),
            manifest_ref: sess_manifest_ref(&child),
            realized: realized.clone(),
            driver: None,
            env_json: Json::Null,
            env_handle_id: None,
            host_caps: Vec::new(),
            turn_active: false,
            active_turn: "turn-1".to_string(),
            finished: false,
            detached: None,
            pendings: Default::default(),
            decided: Default::default(),
            host_asks: Default::default(),
            idem: Default::default(),
            budget_ceiling: Default::default(),
            scan_seq: head.seq,
            next_invoke: None,
            next_completion: String::new(),
            next_response_ref: String::new(),
        };
        self.sessions.insert(session_id.clone(), sess);
        Ok(session_json(
            &session_id,
            &child_run,
            &session_id,
            &sess_manifest_ref(&child),
            &child.configuration_id.clone().unwrap_or_default(),
            &child.configuration_version_id.clone().unwrap_or_default(),
            &realized,
            head.seq as i64,
        ))
    }

    /// `report_host_effect` — the host's report on an
    /// `upcall.invoke_host_capability` ask. The effect's terminal row
    /// mints effect-scoped (`observed{outcome}` | `refused{reason}` |
    /// `unknown{cause}`); a report for an effect the gate never asked
    /// is `UnknownEffect`. `Recorded`.
    pub(crate) fn report_host_effect(&mut self, params: &Json) -> Result<Json, EmbedError> {
        inject::refuse_secrets(params)?;
        let p = ReportHostEffectParams::from_json(params)?;
        {
            let s = self.writer_session(&p.session_id)?;
            if let Some(k) = &p.idempotency_key {
                if let Some(hit) = s.idem.get(k) {
                    return Ok(hit.clone());
                }
            }
            if !s.host_asks.contains_key(&p.effect_id) {
                return Err(EmbedError::UnknownEffect {
                    effect_id: p.effect_id.clone(),
                });
            }
        }
        let (run_id, lease, ask) = {
            let s = self.session(&p.session_id)?;
            let ask = s.host_asks.get(&p.effect_id).cloned().unwrap();
            (
                s.run_id.clone(),
                s.lease.clone().ok_or(EmbedError::Refused {
                    reason: "session_is_read_only".to_string(),
                })?,
                ask,
            )
        };
        let (class, payload) = match &p.outcome {
            HostEffectOutcome::Observed {
                outcome,
                output_artifacts,
            } => (
                "action.effect.observed",
                Json::obj([
                    ("outcome", Json::str(outcome.clone())),
                    (
                        "output_artifacts",
                        Json::Arr(
                            output_artifacts
                                .iter()
                                .map(|a| Json::str(a.clone()))
                                .collect(),
                        ),
                    ),
                    ("reported_by", Json::str("host")),
                ]),
            ),
            HostEffectOutcome::Refused { reason } => (
                "action.effect.refused",
                Json::obj([
                    ("reason", Json::str(reason.clone())),
                    ("reported_by", Json::str("host")),
                ]),
            ),
            HostEffectOutcome::Unknown { detail } => (
                "action.effect.unknown",
                Json::obj([
                    (
                        "cause",
                        Json::str(detail.clone().unwrap_or_else(|| "host_unknown".to_string())),
                    ),
                    ("reported_by", Json::str("host")),
                ]),
            ),
        };
        // The host echoes the attempt it served — a report against a
        // different attempt than the recorded ask is a schema violation.
        if ask.attempt_no != p.attempt_no as u64 {
            return Err(EmbedError::SchemaViolation {
                path: "report_host_effect/attempt_no".to_string(),
                code: "attempt_mismatch".to_string(),
            });
        }
        match self.mint_effect_scoped(
            &run_id,
            &lease,
            class,
            payload,
            &p.effect_id,
            &ask.turn_id,
            &ask.model_call_id,
        ) {
            Ok(()) => {}
            Err(EmbedError::Draining) => return Err(EmbedError::Draining),
            // A second terminal on an already-settled effect — record
            // the host's report as evidence rather than failing the
            // call (the gate's `unknown` row stays the canonical
            // terminal; the report is attached evidence, INV-2).
            Err(_) => self.mint(
                &run_id,
                &lease,
                "host.effect.reported",
                Json::obj([
                    ("effect_id", Json::str(p.effect_id.clone())),
                    ("attempt_no", Json::Int(p.attempt_no)),
                    ("class", Json::str(class)),
                    ("capability_id", Json::str(ask.capability_id.clone())),
                ]),
            )?,
        }
        self.session_mut(&p.session_id)?
            .host_asks
            .remove(&p.effect_id);
        let out = Json::obj([
            ("recorded", Json::Bool(true)),
            ("effect_id", Json::str(p.effect_id.clone())),
        ]);
        if let Some(k) = p.idempotency_key {
            self.session_mut(&p.session_id)?.idem.insert(k, out.clone());
        }
        Ok(out)
    }

    /// `respond_elicitation` — no elicitation channel is opened at
    /// Stage 1 (the ask table is honestly empty); a response against a
    /// never-issued id is refused, never fabricated.
    pub(crate) fn respond_elicitation(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = RespondElicitationParams::from_json(params)?;
        self.writer_session(&p.session_id)?;
        Err(EmbedError::Refused {
            reason: format!("no_pending_elicitation:{}", p.elicitation_id),
        })
    }

    // ── Group R ───────────────────────────────────────────────────────

    /// `stream_events` — open a ledger `Subscription` at `from` and
    /// register the frame adapter (`filter.classes` allowlist ∧
    /// `hello.opt_out_notifications` denylist). `StreamTicket`.
    pub(crate) fn stream_events(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = StreamEventsParams::from_json(params)?;
        let s = self.live_session(&p.session_id)?;
        let run_id = s.run_id.clone();
        let cursor = match &p.from {
            ReadCursor::Seq(n) => Cursor::Seq(*n as u64),
            ReadCursor::EventId(id) => Cursor::EventId(id.clone()),
            ReadCursor::Now => Cursor::Now,
        };
        let head = self.store.head(&run_id).map_err(ledger_err)?;
        let sub = self.store.subscribe(&run_id, cursor).map_err(ledger_err)?;
        let subscription_id = self.alloc("sub");
        let deny = self.client_caps.opt_out_notifications.clone();
        self.subs.insert(
            subscription_id.clone(),
            SubState {
                sub: Some(sub),
                adapter: FrameAdapter::new(p.filter.clone(), deny, cursor_from(&p.from)),
                head_hash: head.hash,
            },
        );
        Ok(StreamTicket { subscription_id }.to_json())
    }

    /// `read` — a durable page (`Cursor::Now` is subscribe-only →
    /// `SchemaViolation`). The class filter is applied post-read
    /// (multi-prefix filters cannot be expressed in `ReadFilter`'s
    /// single `class` slot; the page bound applies to the ledger page).
    pub(crate) fn read_op(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = ReadParams::from_json(params)?;
        let s = self.live_session(&p.session_id)?;
        let cursor = match &p.cursor {
            ReadCursor::Seq(n) => Cursor::Seq(*n as u64),
            ReadCursor::EventId(id) => Cursor::EventId(id.clone()),
            ReadCursor::Now => {
                return Err(EmbedError::SchemaViolation {
                    path: "read/cursor".to_string(),
                    code: "now_is_subscribe_only".to_string(),
                })
            }
        };
        let direction = if p.direction == "rev" {
            ReadDir::Rev
        } else {
            ReadDir::Fwd
        };
        let limit = if p.limit > 0 {
            p.limit as usize
        } else {
            DEFAULT_PAGE
        };
        let page = self
            .store
            .read(&s.run_id, cursor, None, direction, limit)
            .map_err(ledger_err)?;
        let events: Vec<Json> = page
            .events
            .iter()
            .filter(|e| {
                p.filter
                    .as_ref()
                    .map(|f| f.iter().any(|c| e.class.starts_with(c.as_str())))
                    .unwrap_or(true)
            })
            .map(|e| e.to_json())
            .collect();
        Ok(Page {
            events,
            next: page.next_cursor.map(|c| match c {
                Cursor::Seq(n) => ReadCursor::Seq(n as i64),
                Cursor::EventId(id) => ReadCursor::EventId(id),
                Cursor::Now => ReadCursor::Now,
            }),
        }
        .to_json())
    }

    /// `head` — the run's head coordinate.
    pub(crate) fn head_op(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let session_id = session_id_param(params, "head")?;
        let s = self.live_session(&session_id)?;
        let h = self.store.head(&s.run_id).map_err(ledger_err)?;
        Ok(Head {
            seq: h.seq as i64,
            event_id: h.event_id,
            hash: h.hash,
        }
        .to_json())
    }

    /// `project` — the ledger's own view fold (`context_view`,
    /// `run_summary`, `checkpoint`); `view_hash` is the ledger's
    /// (recomputed nowhere — AC-9 byte parity by construction).
    pub(crate) fn project(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = ProjectParams::from_json(params)?;
        let s = self.live_session(&p.session_id)?;
        let kind = match p.view_kind.as_str() {
            "context_view" => ViewKind::ContextView,
            "run_summary" => ViewKind::RunSummary,
            _ => ViewKind::Checkpoint,
        };
        let v = self
            .store
            .project(&s.run_id, kind, p.until_seq.map(|u| u as u64))
            .map_err(ledger_err)?;
        let derived_hash = match v.derived_from_seq {
            Some(seq) => self
                .store
                .events(&s.run_id)
                .ok()
                .and_then(|evs| evs.iter().find(|e| e.seq == seq))
                .map(|e| e.hash.clone())
                .unwrap_or_default(),
            None => String::new(),
        };
        Ok(View {
            payload: v.payload,
            derived_from_seq: v.derived_from_seq.map(|s| s as i64).unwrap_or(-1),
            derived_from_hash: derived_hash,
            view_hash: v.view_hash,
        }
        .to_json())
    }

    /// `account` — the run's `ResourceAccount` projection. Stage 1 runs
    /// the scripted kernel model (no provider usage rows), so the
    /// account is honestly minimal: the ref, the watermark, and the
    /// empty meter table — never fabricated usage.
    pub(crate) fn account(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = AccountParams::from_json(params)?;
        let s = self.live_session(&p.session_id)?;
        let head = self.store.head(&s.run_id).map_err(ledger_err)?;
        let until = p.until_seq.unwrap_or(head.seq as i64);
        Ok(AccountView {
            account: Json::obj([
                ("run_id", Json::str(s.run_id.clone())),
                ("account_ref", Json::str("acct:unbudgeted")),
                ("until_seq", Json::Int(until)),
                ("usage", Json::Arr(vec![])),
            ]),
        }
        .to_json())
    }

    /// `describe` — what the session's open fixed: the manifest ref,
    /// the participant descriptor, the realized settings, and the
    /// environment's connection-info/health/meters (I7: connection
    /// info, never the handle id).
    pub(crate) fn describe(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let session_id = session_id_param(params, "describe")?;
        let s = self.live_session(&session_id)?;
        let manifest = self.store.manifest(&s.run_id).map_err(ledger_err)?.clone();
        let (conn, health, meters) = match &s.env_json {
            Json::Obj(_) => (
                s.env_json
                    .get("connection_info")
                    .cloned()
                    .unwrap_or(Json::Null),
                s.env_json
                    .get("health")
                    .and_then(Json::as_str)
                    .unwrap_or("attached")
                    .to_string(),
                match s.env_json.get("meters") {
                    Some(Json::Arr(a)) => a.clone(),
                    _ => vec![],
                },
            ),
            _ => (Json::Null, "attached".to_string(), vec![]),
        };
        Ok(DescribeResult {
            manifest_ref: s.manifest_ref.clone(),
            participant_descriptor: Json::obj([(
                "class",
                Json::str(manifest.participant_class.as_str()),
            )]),
            protocol_bindings: s.realized.protocol_bindings.clone(),
            realized: s.realized.clone(),
            environment_connection_info: conn,
            environment_health: health,
            environment_meters: meters,
        }
        .to_json())
    }

    /// `list_leases` — the live `ApprovalLease` table. No
    /// `allow_lease` decision lands at Stage 1, so the table is
    /// honestly empty (the record's own doc: empty, not fabricated).
    pub(crate) fn list_leases(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let session_id = session_id_param(params, "list_leases")?;
        self.live_session(&session_id)?;
        Ok(ListLeasesResult { leases: vec![] }.to_json())
    }

    /// `lineage` — the run's `forked_from`/`continued_from` chain,
    /// root-first (the result record is `Page` — the chain entries are
    /// its `events` payload).
    pub(crate) fn lineage(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = LineageParams::from_json(params)?;
        let s = self.live_session(&p.session_id)?;
        let chain = self.store.lineage(&s.run_id).map_err(ledger_err)?;
        Ok(Page {
            events: chain
                .iter()
                .map(|l| {
                    Json::obj([
                        ("run_id", Json::str(l.run_id.clone())),
                        ("up_to_seq", Json::Int(l.up_to_seq as i64)),
                        ("head_hash", Json::str(l.head_hash.clone())),
                    ])
                })
                .collect(),
            next: None,
        }
        .to_json())
    }

    /// `get_artifact` — the blob at `address` (parsed per N1/N2 —
    /// `{algorithm}:{hex}`; a malformed id is a `SchemaViolation`,
    /// never a lookup). Returns `{address, size, content}`; `content`
    /// is UTF-8 lossless when it decodes, hex otherwise.
    pub(crate) fn get_artifact(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = GetArtifactParams::from_json(params)?;
        self.live_session(&p.session_id)?;
        let parsed =
            hh_identity::idp::parse_id(&p.address).map_err(|_| EmbedError::SchemaViolation {
                path: "get_artifact/address".to_string(),
                code: "malformed_id".to_string(),
            })?;
        // `get_blob` keys on the rendered id — a ContentAddress whose
        // media_type/size are unknown is still the same blob.
        let addr = hh_identity::idp::ContentAddress {
            idp: "idp/1",
            algorithm: "sha256",
            digest: parsed.digest_hex.clone(),
            media_type: String::new(),
            size: 0,
        };
        // Content-addressed only: blob addresses hit `blobs/`; a sealed
        // definition's `version_id` hits the artifact table minted at
        // open (both are idp/1 ids — the served bytes hash back to the
        // requested address).
        let bytes = match self.store.get_blob(&addr) {
            Ok(b) => b,
            Err(hh_ledger::errors::LedgerError::Missing { .. }) => {
                self.sealed_defs.get(&p.address).cloned().ok_or_else(|| {
                    ledger_err(hh_ledger::errors::LedgerError::Missing {
                        address: p.address.clone(),
                        reason: hh_ledger::errors::MissingReason::Gc,
                    })
                })?
            }
            Err(e) => return Err(ledger_err(e)),
        };
        let content = match String::from_utf8(bytes.clone()) {
            Ok(t) => Json::str(t),
            Err(_) => Json::str(hex(&bytes)),
        };
        Ok(Json::obj([
            ("address", Json::str(p.address)),
            ("size", Json::Int(bytes.len() as i64)),
            ("content", content),
        ]))
    }
}

/// `{session_id}` — the shared shape of `head`/`describe`/`list_leases`
/// params (declared `*Params` records in the schema; the one field).
fn session_id_param(params: &Json, path: &str) -> Result<String, EmbedError> {
    let mut s = StrictObj::new(params, path)?;
    let id = s.req_str("session_id")?;
    s.finish()?;
    Ok(id)
}

/// The replay start seq for the frame adapter (`from{seq}` → that seq;
/// `event_id`/`now` → `0`, the adapter's `sync.from_seq` is the
/// earliest replay could have covered).
fn cursor_from(c: &ReadCursor) -> i64 {
    match c {
        ReadCursor::Seq(n) => *n,
        _ => 0,
    }
}

/// Lowercase-hex render (the non-UTF-8 `get_artifact` fallback).
fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
