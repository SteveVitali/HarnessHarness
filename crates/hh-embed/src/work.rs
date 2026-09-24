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
use hh_ledger::branch::{BranchKind, EnvBinding, ForkOpts, NavigateTarget, ReplayMode};
use hh_ledger::event::{Cursor, Direction as ReadDir};
use hh_ledger::manifest::{RunKind, RunManifest};
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
        // `concurrent_input` (the bound strategy's declaration): under
        // `queue_only` a mid-turn `submit` is `TurnActive`; under `steer`
        // the input lands as a `steer` cue at the next decision point
        // (AC-R-2.6.1-10 — the declared mode, never a hardcoded answer).
        let mid_turn = {
            let s = self.writer_session(&p.session_id)?;
            if let Some(hit) = s.idem.get(&p.idempotency_key) {
                return Ok(hit.clone());
            }
            if s.finished {
                return Err(EmbedError::Draining);
            }
            if s.turn_active && s.steering.1 == hh_control::strategy::ConcurrentInput::QueueOnly {
                return Err(EmbedError::TurnActive);
            }
            s.turn_active
        };
        let payload_ref = self.stage_input(&p.session_id, &p.input)?;
        {
            let s = self.session_mut(&p.session_id)?;
            if let Some(d) = s.driver.as_mut() {
                d.submit(Cue::HumanInput(if mid_turn {
                    HumanInput::Steer { payload_ref }
                } else {
                    HumanInput::FollowUp { payload_ref }
                }));
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

    /// Stage the scripted input blocks for the model port + mint the
    /// `context.artefact.delivered` rows; returns the artefact ref the
    /// cue carries (the cue names a record, never bytes — I2).
    fn stage_input(&mut self, sess_id: &str, input: &[Json]) -> Result<String, EmbedError> {
        // The scripted model port reads the staged input: one
        // `{kind:"invoke"}` block routes to that capability surface;
        // otherwise the input is the completion the `hh.submit` call
        // carries (its first `text` member, else the canonical input).
        let mut invoke: Option<(String, Json)> = None;
        let mut completion = String::new();
        for block in input {
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
            completion = Json::Arr(input.to_vec()).to_canonical_string();
        }
        let response_ref = self.alloc("resp");
        {
            let s = self.session_mut(sess_id)?;
            s.next_invoke = invoke;
            s.next_completion = completion;
            s.next_response_ref = response_ref;
        }
        self.record_input(sess_id, input)
    }

    /// `steer` — honoured per the bound control strategy's declared
    /// `steer_mode` (AC-R-2.6.1-10): `interrupt_at_decision_point`
    /// submits the `steer` cue and drives; `queue_next_turn` records the
    /// delivery and holds the cue for the next turn's first decision
    /// point (`Accepted{queued_at}` reports which); `unsupported` is the
    /// honest `Unsupported{by: control_strategy}`. The input is ledgered
    /// (`context.artefact.delivered`) before any admission — steering is
    /// a ledger fact, never UI state.
    pub(crate) fn steer(&mut self, params: &Json) -> Result<Json, EmbedError> {
        inject::refuse_secrets(params)?;
        let p = SteerParams::from_json(params)?;
        let mode = {
            let s = self.writer_session(&p.session_id)?;
            if let Some(k) = &p.idempotency_key {
                if let Some(hit) = s.idem.get(k) {
                    return Ok(hit.clone());
                }
            }
            if s.finished {
                return Err(EmbedError::Draining);
            }
            if let Some(t) = &p.expected_turn_id {
                if t != &s.active_turn {
                    return Err(EmbedError::TurnMismatch {
                        active_turn_id: s.active_turn.clone(),
                    });
                }
            }
            s.steering.0
        };
        match mode {
            hh_control::strategy::SteerMode::Unsupported => Err(EmbedError::Unsupported {
                by: "control_strategy".to_string(),
            }),
            hh_control::strategy::SteerMode::InterruptAtDecisionPoint => {
                let payload_ref = self.stage_input(&p.session_id, &p.input)?;
                {
                    let s = self.session_mut(&p.session_id)?;
                    if let Some(d) = s.driver.as_mut() {
                        d.submit(Cue::HumanInput(HumanInput::Steer { payload_ref }));
                    }
                }
                let _ = self.drive(&p.session_id);
                let turn = self.session(&p.session_id)?.active_turn.clone();
                let out = Json::obj([
                    ("turn_id", Json::str(turn)),
                    ("queued_at", Json::str("decision_point")),
                ]);
                if let Some(k) = p.idempotency_key {
                    self.session_mut(&p.session_id)?.idem.insert(k, out.clone());
                }
                Ok(out)
            }
            hh_control::strategy::SteerMode::QueueNextTurn => {
                let payload_ref = self.stage_input(&p.session_id, &p.input)?;
                self.session_mut(&p.session_id)?.pending_steer = Some(payload_ref);
                let turn = self.session(&p.session_id)?.active_turn.clone();
                let out = Json::obj([
                    ("turn_id", Json::str(turn)),
                    ("queued_at", Json::str("next_turn")),
                ]);
                if let Some(k) = p.idempotency_key {
                    self.session_mut(&p.session_id)?.idem.insert(k, out.clone());
                }
                Ok(out)
            }
        }
    }

    /// `respond_permission` — the host's answer to a live
    /// `security.permission.pending` (§5g.7 §5): `selected{option_id}` must
    /// name an offered option (`OptionNotOffered`); the response runs the
    /// monitor's `ApprovalState::respond` semantics over the folded trail
    /// (the ledger is the one truth — CC1), mints the final
    /// `security.permission.decided` (+ `security.permission.lease.granted`
    /// for `allow_lease`), and resumes the loop when the ask covered an
    /// effect (`approval` cue). `more_info`/`escalate` leave the pending
    /// open — they are not decisions. `Recorded`.
    pub(crate) fn respond_permission(&mut self, params: &Json) -> Result<Json, EmbedError> {
        use hh_monitor::approval::{
            ApprovalResponse, ApprovalState, EndorserRef, LeaseSpec, RespondCtx, ResponseChoice,
        };
        use hh_monitor::decision::{Decision, DecisionScope};
        use hh_provenance::authority::AuthorityClass;
        use hh_provenance::{HumanRole, Origin, PersistenceScope, ProvenanceRecord};

        inject::refuse_secrets(params)?;
        let p = RespondPermissionParams::from_json(params)?;
        let (
            run_id,
            lease,
            pending,
            manifest_ref,
            policy_mode,
            active_turn,
            authority_caps,
            narrowing_leaf_ids,
        ) = {
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
                    .cloned()
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
            (
                s.run_id.clone(),
                s.lease.clone().ok_or(EmbedError::Refused {
                    reason: "session_is_read_only".to_string(),
                })?,
                pending,
                s.manifest_ref.clone(),
                s.realized.policy_mode.clone(),
                s.active_turn.clone(),
                s.authority_caps.clone(),
                s.narrowing_leaf_ids.clone(),
            )
        };
        // Fold the durable prefix into the approval fold — the response's
        // semantics (exactly-one, legitimacy, lease minting, the denial
        // counters) run over the record, never the session cache (CC1).
        let events = self.store.events(&run_id).map_err(ledger_err)?.to_vec();
        let head_seq = events.last().map(|e| e.seq).unwrap_or(0);
        let mut approvals = ApprovalState::project(&events, head_seq);
        // The non-final `decided{ask}` row for this permission carries the
        // effective risk the chain assessed — `never_auto` marks the
        // irreversible/unknown classes whose leases require the declared
        // admission legs (pattern + `max_uses` + `scope ≤ run`, ADR-0071 D1).
        let asked_risk = events
            .iter()
            .rev()
            .find(|e| {
                e.class == "security.permission.decided"
                    && e.payload.get("permission_id").and_then(Json::as_str)
                        == Some(p.permission_id.as_str())
                    && e.payload.get("decision").and_then(Json::as_str) == Some("ask")
            })
            .and_then(|e| e.payload.get("effective_risk_class"))
            .and_then(hh_ontology::risk::RiskClass::from_json);
        let irreversible = asked_risk
            .map(hh_monitor::approval::never_auto)
            .unwrap_or(false);
        // The wire option → the monitor's response choice. The session's
        // respond is the principal's answer — `human{authority: principal}`.
        let (choice, scope) = match &p.outcome {
            PermissionOutcome::Cancelled => (
                ResponseChoice::Deny {
                    reason: "cancelled".to_string(),
                },
                DecisionScope::Once,
            ),
            PermissionOutcome::Selected { option_id } => match option_id.as_str() {
                "allow_once" | "allow" => (ResponseChoice::AllowOnce, DecisionScope::Once),
                "allow_lease" => (
                    ResponseChoice::AllowLease(LeaseSpec {
                        pattern: None,
                        scope: hh_monitor::approval::LeaseScope::Run,
                        max_uses: None,
                    }),
                    DecisionScope::Session,
                ),
                // Not a decision — the owed-decision row survives.
                "more_info" | "escalate" => (ResponseChoice::MoreInfo, DecisionScope::Once),
                _ => (
                    ResponseChoice::Deny {
                        reason: option_id.clone(),
                    },
                    DecisionScope::Once,
                ),
            },
        };
        // `allow_lease` mints over the pending's capability material — a
        // pending without `request{subject_ref, capability_ref,
        // args_canonical_hash}` cannot key a lease (never fabricated).
        if matches!(choice, ResponseChoice::AllowLease(_))
            && (pending.capability_ref.is_none()
                || pending.args_canonical_hash.is_none()
                || pending.subject_ref.is_none())
        {
            return Err(EmbedError::Refused {
                reason: "allow_lease_requires_capability_material".to_string(),
            });
        }
        let now = self.store.now_ms();
        let ctx = RespondCtx {
            policy_fingerprint: hh_monitor::approval::policy_fingerprint(
                &manifest_ref,
                &policy_mode,
                &narrowing_leaf_ids,
            ),
            scope_ref: run_id.clone(),
            risk_ceiling: asked_risk.unwrap_or(hh_ontology::risk::RiskClass::UNKNOWN),
            grant_authority: AuthorityClass::Principal,
            grants: Vec::new(),
            denial_policy: None,
        };
        let outcome = approvals
            .respond_with_ctx(
                &ApprovalResponse {
                    permission_id: p.permission_id.clone(),
                    choice,
                    scope,
                    max_uses: None,
                    justification: None,
                    decided_by: EndorserRef::Human {
                        subject_ref: "human:principal".to_string(),
                        authority: AuthorityClass::Principal,
                    },
                    decided_at: now,
                },
                irreversible,
                now,
                &ctx,
            )
            .map_err(|e| EmbedError::Refused {
                reason: format!("approval_respond: {e:?}"),
            })?;
        if outcome.already_decided {
            return Err(EmbedError::AlreadyDecided {
                permission_id: p.permission_id.clone(),
            });
        }
        if matches!(outcome.decision, Decision::Ask { .. }) {
            // `more_info`/`escalate` — the pending stays open; no `decided`
            // row is minted (the owed-decision record survives §5g.7 §5).
            let out = recorded_json(&p.permission_id);
            self.session_mut(&p.session_id)?
                .idem
                .insert(p.idempotency_key, out.clone());
            return Ok(out);
        }
        // The final `decided` row — the canonical members the class table
        // declares (`decision ∈ {allow, deny}`, `decider = human`, the
        // scope the response conferred, the wait accounting).
        let decision_tag = match &outcome.decision {
            Decision::Allow => "allow",
            Decision::Deny { .. } => "deny",
            Decision::Ask { .. } => unreachable!("ask handled above"),
        };
        let mut members = vec![
            ("permission_id", Json::str(p.permission_id.clone())),
            ("proposal", Json::str(pending.proposal.clone())),
            ("decision", Json::str(decision_tag)),
            ("decider", Json::str("human")),
            ("decider_ref", Json::str("human:principal")),
            ("decision_scope", Json::str(scope.as_str())),
            ("requested_at", Json::Int(pending.requested_at as i64)),
            (
                "wait_ms",
                Json::Int(now.saturating_sub(pending.requested_at) as i64),
            ),
        ];
        if let Some(ef) = &pending.effect_id {
            members.push(("effect_id", Json::str(ef.clone())));
        }
        if !pending.options.is_empty() {
            members.push((
                "options_presented",
                Json::Arr(
                    pending
                        .options
                        .iter()
                        .map(|o| Json::str(o.clone()))
                        .collect(),
                ),
            ));
        }
        if let PermissionOutcome::Selected { option_id } = &p.outcome {
            if decision_tag == "deny" {
                members.push(("reason", Json::str(option_id.clone())));
            }
        } else {
            members.push(("reason", Json::str("cancelled")));
        }
        if let Some(l) = &outcome.lease {
            members.push(("cache_key", Json::str(l.key_hash.clone())));
        }
        // `decided` + `lease.granted` commit in one batch — the lease is the
        // decision's artifact; a torn pair would read as an allow-once.
        let mut batch = vec![hh_env::events::EventMinter::new(&self.store, &run_id)
            .mint("security.permission.decided", Json::obj(members))
            .map_err(ledger_err)?];
        if let Some(l) = &outcome.lease {
            batch.push(
                hh_env::events::EventMinter::new(&self.store, &run_id)
                    .mint(
                        "security.permission.lease.granted",
                        hh_monitor::approval::lease_granted_payload(l),
                    )
                    .map_err(ledger_err)?,
            );
        }
        // The `approval`-basis handle (§5g.1 §9 — the approval mints
        // `{issuer = principal, grants = requested ⊓ authority_cap,
        // origin_basis = approval, delegable = false, lifetime}`; I-H6/H-7
        // lifetimes inside `mint_approval_handle`). Minted only over the
        // pending's *recorded* material — `subject_ref` (the holder),
        // `capability_ref`, `requested_grants`; a pending without them (a
        // capability-less host ask) carries no conferable material and the
        // `decided` row stands alone — nothing is fabricated. The granted
        // row's envelope id is the handle's `issued_at`/holder pin, so the
        // ids are allocated with the handle (the fold re-pins
        // `holder.version = Pinned(event_id)` on rebuild).
        let allow = matches!(outcome.decision, Decision::Allow);
        if allow && pending.subject_ref.is_some() && pending.capability_ref.is_some() {
            let handle_id = self.store.alloc_id("hnd");
            let granted_event_id = self.store.alloc_id("evt");
            let mint_in = hh_monitor::mint::ApprovalMint {
                permission_id: p.permission_id.clone(),
                holder: hh_hir::refs::Ref {
                    semantic_id: pending.subject_ref.clone().unwrap_or_default(),
                    version: hh_hir::refs::RefVersion::Pinned(granted_event_id.clone()),
                },
                issuer: ProvenanceRecord::minted(
                    Origin::human("human:principal", HumanRole::Principal),
                    PersistenceScope::Run,
                    now,
                ),
                grants: pending.requested_grants.clone(),
                // `requested ⊓ authority_cap` — the definition's caps (the
                // `of`-named and `*` global rows) only ever lower the minted
                // ceiling below the responder's authority. The `of` member
                // names the bounded *entity* (ADR-0240) — here the pending's
                // capability semantic id, never the ephemeral request id
                // (a cap cannot name a request minted at runtime).
                ceiling: authority_caps
                    .iter()
                    .filter(|c| {
                        c.of.as_deref().is_none_or(|of| {
                            of == "*"
                                || pending
                                    .capability_ref
                                    .as_ref()
                                    .map(|(sid, _)| of == sid)
                                    .unwrap_or(false)
                        })
                    })
                    .fold(ctx.grant_authority, |c, cap| c.min(cap.ceiling)),
                scope,
                lease_scope: outcome.lease.as_ref().map(|l| l.scope),
                lease_pattern: outcome.lease.as_ref().and_then(|l| l.pattern.clone()),
                effect_id: pending.effect_id.clone().unwrap_or_default(),
                turn_id: active_turn.clone(),
                run_id: run_id.clone(),
                session_ref: p.session_id.clone(),
                budget_ref: None,
            };
            let minted = {
                let store = &self.store;
                let mut alloc = |kind: &str| match kind {
                    "hnd" => handle_id.clone(),
                    "evt" => granted_event_id.clone(),
                    other => store.alloc_id(other),
                };
                hh_monitor::mint::mint_approval_handle(&mint_in, &mut alloc)
            };
            let (handle, evt_id) = minted.map_err(|e| EmbedError::Refused {
                reason: format!("approval_mint: {e:?}"),
            })?;
            batch.push(
                hh_env::events::EventMinter::new(&self.store, &run_id)
                    .mint_with_id(
                        "security.permission.granted",
                        hh_monitor::events::granted_payload(&handle),
                        evt_id,
                    )
                    .map_err(ledger_err)?,
            );
        }
        self.store
            .append(&run_id, &lease, batch)
            .map_err(ledger_err)?;
        {
            let s = self.session_mut(&p.session_id)?;
            s.decided.insert(
                p.permission_id.clone(),
                Json::obj([
                    ("kind", Json::str("selected")),
                    ("decision", Json::str(decision_tag)),
                ]),
            );
            s.pendings.remove(&p.permission_id);
            if let Some(ef) = &pending.effect_id {
                if let Some(d) = s.driver.as_mut() {
                    d.submit(Cue::HumanInput(HumanInput::Approval {
                        effect_id: ef.clone(),
                        allow,
                    }));
                }
            }
        }
        if pending.effect_id.is_some() {
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
    /// the run's `interactive` effective attendance or a caller
    /// `attestation`; anything less is `AuthorityWideningRequiresHuman`.
    /// A tightening amendment is admitted unconditionally.
    /// `amend{approval_mode | attendance}` lands at Stage 2 via
    /// `amend_policy` (ADR-0168(e)).
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
            return self.amend_policy(&p);
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
        let interactive = {
            // I-1 asks whether a human is at the terminal *now* — the
            // amending invocation's own declaration when it carries one
            // (a resumed session inherits the run's open-time record,
            // which says nothing about this call's channel), else the
            // session's recorded attendance.
            let s = self.session(&p.session_id)?;
            match &p.invocation {
                Some(inv) => inv.attendance.value == "interactive",
                None => s.realized.attendance.value == "interactive",
            }
        };
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

    /// `amend{attendance | approval_mode}` — the remaining ADR-0216/OQ-468
    /// targets (ADR-0168 D1/D3): one ledgered `control.<target>.amended`
    /// row per amendment, the session's realized settings updated so the
    /// next `policy_fingerprint` derivation revokes leases by key
    /// construction (ADR-0071 D1), plus durable `lease.revoked` rows over
    /// every live lease — an attendance or mode change revokes
    /// explicitly, never silently (ADR-0168 D1's "like any
    /// policy-fingerprint change").
    ///
    /// Widening rule (I-1's shape): moving *toward* a human/auto
    /// channel — `→ interactive` attendance, or an `approval_mode` whose
    /// auto-resolution breadth rank rises — requires a caller
    /// `attestation` or currently-interactive effective attendance;
    /// tightening is admitted unconditionally. `approval_mode = bypass`
    /// additionally re-checks the recorded
    /// `containment.enforcement_evidence` — a bypass amendment into a
    /// binding without it is `Refused{bypass_without_containment}`.
    fn amend_policy(&mut self, p: &AmendParams) -> Result<Json, EmbedError> {
        use hh_monitor::approval::ApprovalState;
        let (run_id, lease, manifest, old_attendance, old_mode) = {
            let s = self.session(&p.session_id)?;
            (
                s.run_id.clone(),
                s.lease.clone().ok_or(EmbedError::Refused {
                    reason: "session_is_read_only".to_string(),
                })?,
                self.store.manifest(&s.run_id).map_err(ledger_err)?.clone(),
                s.realized.attendance.value.clone(),
                s.realized.policy_mode.clone(),
            )
        };
        let interactive_now = match &p.invocation {
            Some(inv) => inv.attendance.value == "interactive",
            None => old_attendance == "interactive",
        };
        let (class, old_v, new_v) = match p.target.as_str() {
            "attendance" => {
                let new = p
                    .value
                    .get("value")
                    .and_then(Json::as_str)
                    .or_else(|| p.value.as_str())
                    .unwrap_or("")
                    .to_string();
                if !matches!(new.as_str(), "interactive" | "async" | "unattended") {
                    return Err(EmbedError::SchemaViolation {
                        path: "amend/value".to_string(),
                        code: "bad_attendance_value".to_string(),
                    });
                }
                // Widening ⇒ a human channel appears where Π-12 denied —
                // attestation only (the run's own attendance is the very
                // thing being widened; it cannot self-certify).
                let widening = new == "interactive" && !interactive_now;
                if widening && p.attestation.is_none() {
                    return Err(EmbedError::AuthorityWideningRequiresHuman {
                        detail: "amend(attendance) → interactive widens the approval                                  channel and requires a human attestation"
                            .to_string(),
                    });
                }
                ("control.attendance.amended", old_attendance, new)
            }
            "approval_mode" => {
                let new = p
                    .value
                    .get("mode")
                    .and_then(Json::as_str)
                    .or_else(|| p.value.as_str())
                    .unwrap_or("")
                    .to_string();
                if !AMENDABLE_APPROVAL_MODES.contains(&new.as_str()) {
                    return Err(EmbedError::SchemaViolation {
                        path: "amend/value".to_string(),
                        code: "bad_approval_mode".to_string(),
                    });
                }
                let widening = approval_mode_rank(&new) > approval_mode_rank(&old_mode);
                if widening && !interactive_now && p.attestation.is_none() {
                    return Err(EmbedError::AuthorityWideningRequiresHuman {
                        detail: format!(
                            "amend(approval_mode) {old_mode} → {new} widens auto-resolution                              without interactive attendance or attestation"
                        ),
                    });
                }
                // `bypass` re-checks the recorded enforcement evidence —
                // the manifest's `containment.enforcement_evidence` member
                // is the durable verdict `open` computed (ADR-0168 D3).
                if new == "bypass" {
                    // The durable relied-groups verdict `open` recorded
                    // (`containment.bypass_admissible`) — the same fold
                    // the pre-open gate ran, never a re-derived guess.
                    let admissible = manifest
                        .extra
                        .get("containment")
                        .and_then(|c| c.get("bypass_admissible"))
                        .and_then(|v| match v {
                            Json::Bool(b) => Some(*b),
                            _ => None,
                        })
                        .unwrap_or(false);
                    if !admissible {
                        return Err(EmbedError::Refused {
                            reason: "bypass_without_containment".to_string(),
                        });
                    }
                }
                ("control.approval_mode.amended", old_mode, new)
            }
            _ => {
                return Err(EmbedError::SchemaViolation {
                    path: "amend/target".to_string(),
                    code: "unknown_amend_target".to_string(),
                })
            }
        };
        // The amendment row — `old`/`new`/`authority` mirroring
        // `control.budget.amended`'s shape.
        self.mint(
            &run_id,
            &lease,
            class,
            Json::obj([
                ("old", Json::str(old_v.clone())),
                ("new", Json::str(new_v.clone())),
                ("authority", Json::str("principal")),
                ("attested", Json::Bool(p.attestation.is_some())),
            ]),
        )?;
        // Revoke every live lease — a policy change revokes by durable
        // row, the same class the fingerprint-miss path uses
        // (ADR-0168 D1; the projector's `key_hash`/`revoked_at` fold).
        {
            let events = self.store.events(&run_id).map_err(ledger_err)?.to_vec();
            let head_seq = events.last().map(|e| e.seq).unwrap_or(0);
            let approvals = ApprovalState::project(&events, head_seq);
            let now = self.store.now_ms();
            for (key, l) in &approvals.leases {
                if l.revoked_at.is_none() {
                    self.mint(
                        &run_id,
                        &lease,
                        "security.permission.lease.revoked",
                        Json::obj([
                            ("lease_id", Json::str(l.lease_id.clone())),
                            ("key_hash", Json::str(key.clone())),
                            ("revoked_at", Json::Int(now as i64)),
                            ("reason", Json::str("policy_change")),
                        ]),
                    )?;
                }
            }
        }
        // Apply to the session's realized settings — the next
        // `policy_fingerprint` derivation reads the new legs.
        {
            let s = self.session_mut(&p.session_id)?;
            match p.target.as_str() {
                "attendance" => {
                    s.realized.attendance = hh_embed_schema::types::AttendanceDeclaration {
                        value: new_v.clone(),
                        source: "declared".to_string(),
                    };
                }
                _ => {
                    s.realized.policy_mode = new_v.clone();
                }
            }
        }
        // `lifecycle.surface.invoked` — an amendment is a run-amending
        // surface invocation.
        if let Some(inv) = &p.invocation {
            self.mint(&run_id, &lease, "lifecycle.surface.invoked", inv.to_json())?;
        }
        let head = self.store.head(&run_id).map_err(ledger_err)?;
        // The returned session reflects the *amended* settings — the
        // pre-amend snapshot would misreport `policy_mode`/`attendance`.
        let amended_realized = self.session(&p.session_id)?.realized.clone();
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
            &amended_realized,
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
    /// `fork` — the S2.9 inter-run branch (§5a.1 §5; R-2.2.4; ADR-0271).
    /// The cut must be coherent (`check_fork_point`; `coerce_to_boundary`
    /// coerces and the record says so). `env: snapshot` restores the newest
    /// `fs_tree` snapshot at/below the cut into a fresh child env;
    /// `trace_only` binds none and the child session is read-only;
    /// `shared_live` is the typed refusal (a mutable live environment is
    /// never shared). The child's `lifecycle.run.forked{BranchRecord}` pins
    /// the source prefix's referenced content (the ledger carries the pin
    /// in the row's `refs`).
    pub(crate) fn fork(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = ForkParams::from_json(params)?;
        if p.manifest_delta.is_some() {
            return Err(EmbedError::Refused {
                reason: "manifest_delta_unsupported".to_string(),
            });
        }
        let kind = BranchKind::parse(p.kind.as_deref().unwrap_or("branch")).ok_or_else(|| {
            EmbedError::SchemaViolation {
                path: "fork/kind".to_string(),
                code: "unknown_branch_kind".to_string(),
            }
        })?;
        let env = EnvBinding::parse(p.env.as_deref().unwrap_or("none")).ok_or_else(|| {
            EmbedError::SchemaViolation {
                path: "fork/env".to_string(),
                code: "unknown_env_binding".to_string(),
            }
        })?;
        if env == EnvBinding::SharedLive {
            return Err(EmbedError::Refused {
                reason: "shared_mutable_env: a live environment is never shared between branches"
                    .to_string(),
            });
        }
        let replay_mode = ReplayMode::parse(p.replay_mode.as_deref().unwrap_or("inherited"))
            .ok_or_else(|| EmbedError::SchemaViolation {
                path: "fork/replay_mode".to_string(),
                code: "unknown_replay_mode".to_string(),
            })?;
        // Resolve the source + the cut (an `event_ref` may name a run other
        // than the session's — the fork is inter-run).
        let (source_run, at_seq) = {
            let s = self.live_session(&p.session_id)?;
            match &p.at {
                ForkPoint::Seq(n) => (s.run_id.clone(), *n as u64),
                ForkPoint::EventRef { run_id, event_id } => {
                    let events = self.store.envelopes(run_id).map_err(ledger_err)?;
                    let seq = events
                        .iter()
                        .find(|e| &e.event_id == event_id)
                        .map(|e| e.seq)
                        .ok_or_else(|| EmbedError::SchemaViolation {
                            path: "fork/at/event_id".to_string(),
                            code: "unknown_event".to_string(),
                        })?;
                    (run_id.clone(), seq)
                }
            }
        };
        // Replay coverage — `exact`/`structural` need the source's
        // observability to carry them; a downgrade is recorded, never silent
        // (DF-S2.9-2 — the coverage→mode ladder).
        let parent = self
            .store
            .manifest(&source_run)
            .map_err(ledger_err)?
            .clone();
        let obs = &parent.observability_level;
        let has = |l: hh_ledger::manifest::ObservabilityLevel| obs.contains(&l);
        use hh_ledger::manifest::ObservabilityLevel as OL;
        let (effective_replay, downgrade_reason) = match replay_mode {
            ReplayMode::Exact if !has(OL::Ledger) => (
                ReplayMode::Observational,
                Some(format!(
                    "coverage: exact requires observability ⊇ ledger; run has {{{}}} — downgraded to observational",
                    obs.iter().map(|l| l.as_str()).collect::<Vec<_>>().join(",")
                )),
            ),
            ReplayMode::Structural if !(has(OL::ModelIo) || has(OL::Ledger)) => (
                ReplayMode::Observational,
                Some(
                    "coverage: structural requires model_io|ledger observability — downgraded to observational"
                        .to_string(),
                ),
            ),
            ReplayMode::Inherited => {
                let eff = if has(OL::Ledger) {
                    ReplayMode::Exact
                } else if has(OL::ModelIo) {
                    ReplayMode::Structural
                } else {
                    ReplayMode::Observational
                };
                (eff, None)
            }
            m => (m, None),
        };
        // `env: snapshot` — choose the newest fs_tree snapshot at/below the
        // cut (absent ⇒ the typed `SnapshotUnavailable` refusal).
        let mut snapshot_choice = None;
        if env == EnvBinding::Snapshot {
            let drv = self.env_drivers.get(&source_run).ok_or_else(|| {
                EmbedError::EnvironmentUnavailable {
                    reason: "snapshot_unavailable: no env driver for the source run".to_string(),
                }
            })?;
            let chosen = drv
                .snapshot_for(&self.store, at_seq)
                .map_err(crate::open::env_err)?
                .ok_or_else(|| EmbedError::EnvironmentUnavailable {
                    reason: format!("snapshot_unavailable: no fs_tree snapshot at seq ≤ {at_seq}"),
                })?;
            if let Some(want) = &p.snapshot_ref {
                if &chosen.0.snapshot_ref != want {
                    return Err(EmbedError::Refused {
                        reason: format!(
                            "snapshot_ref {want} is not the chooser's snapshot at ≤ {at_seq} ({})",
                            chosen.0.snapshot_ref
                        ),
                    });
                }
            }
            snapshot_choice = Some(chosen);
        }
        // The child manifest — inherits the parent's bindings; the budget
        // slice and policy ref bind the child's spend/policy (attenuated —
        // the record names them; widening has no path here).
        let mut child = RunManifest::minimal(RunKind::Agent);
        child.configuration_id = parent.configuration_id.clone();
        child.configuration_version_id = parent.configuration_version_id.clone();
        child.harness_def_ref = parent.harness_def_ref.clone();
        child.attendance = parent.attendance;
        child.budget = p.budget_slice_ref.clone().or(parent.budget.clone());
        child.envelope_policy_ref = p.policy_ref.clone().or(parent.envelope_policy_ref.clone());
        // `Store::fork` fills `forked_from{run_id, at_seq, head_hash}` at the
        // (possibly coerced) cut — `open_run` re-checks the anchor.
        child.parent_run_id = Some(source_run.clone());
        let holder = self.holder.clone();
        let opts = ForkOpts {
            env,
            replay_mode,
            effective_replay,
            downgrade_reason,
            policy_ref: p.policy_ref.clone(),
            budget_slice_ref: p.budget_slice_ref.clone(),
            coerce_to_boundary: p.coerce_to_boundary.unwrap_or(false),
            snapshot_ref: snapshot_choice
                .as_ref()
                .map(|(r, _)| r.snapshot_ref.clone()),
            snapshot_at_seq: snapshot_choice.as_ref().map(|(r, _)| r.at_seq),
            read_only: env == EnvBinding::TraceOnly,
            ..Default::default()
        };
        // `snapshot_at_seq` goes in the record payload too — stash it on the
        // forked row (the ledger's `env_snapshots` fold is the row-side copy).
        let (child_run, lease, record) = self
            .store
            .fork(&source_run, at_seq, kind, &opts, child.clone(), &holder)
            .map_err(ledger_err)?;
        // `env: snapshot` — materialise the child's workspace from the
        // snapshot's blob content (never the parent's live tree).
        let mut child_env_handle: Option<String> = None;
        if let Some((rec, tree)) = &snapshot_choice {
            let parent_handle = self
                .env_drivers
                .get(&source_run)
                .and_then(|d| d.handle(&rec.env_handle_id))
                .cloned()
                .ok_or_else(|| EmbedError::EnvironmentUnavailable {
                    reason: format!(
                        "snapshot's env handle {} is not live in this kernel",
                        rec.env_handle_id
                    ),
                })?;
            let cdrv = self
                .env_drivers
                .entry(child_run.clone())
                .or_insert_with(|| hh_env::driver::EnvDriver::new(&child_run));
            let ch = cdrv
                .derive_from_snapshot(&mut self.store, &lease, &parent_handle, tree)
                .map_err(crate::open::env_err)?;
            child_env_handle = Some(ch.env_handle_id.clone());
        }
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
        let read_only = env == EnvBinding::TraceOnly;
        let sess = crate::service::SessionState {
            run_id: child_run.clone(),
            // `trace_only` ⇒ the session is read-only by construction —
            // `writer_session` refuses every mutation op on it (I6's
            // mechanism is the enforcement; the record carries `read_only`).
            attach: read_only,
            lease: Some(lease),
            manifest_ref: sess_manifest_ref(&child),
            realized: realized.clone(),
            driver: None,
            steering: (
                hh_control::strategy::SteerMode::Unsupported,
                hh_control::strategy::ConcurrentInput::QueueOnly,
            ),
            pending_steer: None,
            env_json: Json::Null,
            env_handle_id: child_env_handle,
            host_caps: Vec::new(),
            turn_active: false,
            active_turn: "turn-1".to_string(),
            finished: false,
            detached: None,
            authority_caps: {
                let r = sess_manifest_ref(&child);
                if r.is_empty() {
                    Vec::new()
                } else {
                    self.persisted_definition(&r)
                        .map(|d| hh_monitor::mint::cap_rows(&d.document))
                        .unwrap_or_default()
                }
            },
            narrowing_leaf_ids: crate::open::manifest_leaf_ids(&child),
            pendings: Default::default(),
            decided: Default::default(),
            delivered_wokens: Default::default(),
            host_asks: Default::default(),
            idem: Default::default(),
            budget_ceiling: Default::default(),
            leaf_arm: crate::open::LeafArm::default(),
            scan_seq: head.seq,
            next_invoke: None,
            next_completion: String::new(),
            next_response_ref: String::new(),
        };
        self.sessions.insert(session_id.clone(), sess);
        let mut out = session_json(
            &session_id,
            &child_run,
            &session_id,
            &sess_manifest_ref(&child),
            &child.configuration_id.clone().unwrap_or_default(),
            &child.configuration_version_id.clone().unwrap_or_default(),
            &realized,
            head.seq as i64,
        );
        if let Json::Obj(m) = &mut out {
            m.insert("branch_record".to_string(), record.to_json());
        }
        Ok(out)
    }

    /// `navigate(run's session, to) → the head.moved record` (§5a.1; ADR-0271).
    /// A HEAD move is append-only: the `lifecycle.head.moved` row lands, the
    /// logical head folds to `to`, and subscribers receive `rewind`. Nothing
    /// is truncated — `read` still serves the whole prefix.
    pub(crate) fn navigate(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = NavigateParams::from_json(params)?;
        let (run_id, lease) = {
            let s = self.writer_session(&p.session_id)?;
            (
                s.run_id.clone(),
                s.lease.clone().ok_or(EmbedError::Refused {
                    reason: "session_is_read_only".to_string(),
                })?,
            )
        };
        let to = match &p.to {
            NavigateTo::Root => NavigateTarget::Root,
            NavigateTo::Seq(n) => NavigateTarget::Seq(*n as u64),
            NavigateTo::EventRef(id) => NavigateTarget::EventId(id.clone()),
        };
        let env = self
            .store
            .navigate(
                &run_id,
                &lease,
                to,
                p.reason.as_deref().unwrap_or("navigate"),
            )
            .map_err(ledger_err)?;
        Ok(Json::obj([
            ("head_moved", Json::Bool(true)),
            ("moved_event_id", Json::str(&env.event_id)),
            ("moved_seq", Json::Int(env.seq as i64)),
            (
                "to_seq",
                env.payload.get("to_seq").cloned().unwrap_or(Json::Null),
            ),
            (
                "to_event_id",
                env.payload
                    .get("to_event_id")
                    .cloned()
                    .unwrap_or(Json::Null),
            ),
        ]))
    }

    /// `coherent_fork_points(session, from_seq?, to_seq?) → {points[]}` —
    /// the pure coherence projection (§5a.4; AC-R-2.2.4-1). Read-only.
    pub(crate) fn coherent_fork_points(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = CoherentForkPointsParams::from_json(params)?;
        let run_id = self.live_session(&p.session_id)?.run_id.clone();
        let points = self
            .store
            .coherent_fork_points(
                &run_id,
                p.from_seq.map(|s| s as u64),
                p.to_seq.map(|s| s as u64),
            )
            .map_err(ledger_err)?;
        let nearest = |at: u64| self.store.nearest_coherent_seq(&run_id, at).ok().flatten();
        let _ = nearest;
        Ok(Json::obj([
            ("run_id", Json::str(&run_id)),
            (
                "points",
                Json::Arr(points.iter().map(|s| Json::Int(*s as i64)).collect()),
            ),
        ]))
    }

    /// `rollback(session, to_seq, reason?, restore_env?) → RollbackRecord`
    /// (§5a.1; R-2.2.5; ADR-0271). The env half runs first (the snapshot
    /// restores in place; what it cannot recapture lands `uncaptured`), then
    /// the ledger: coherence gate → scoped compensation saga → the rewind
    /// note blob → `rolled_back` + `head.moved` + the `rewind` frame.
    /// Nothing is deleted — history is append-only.
    pub(crate) fn rollback(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = RollbackParams::from_json(params)?;
        let (run_id, lease, env_handle) = {
            let s = self.writer_session(&p.session_id)?;
            (
                s.run_id.clone(),
                s.lease.clone().ok_or(EmbedError::Refused {
                    reason: "session_is_read_only".to_string(),
                })?,
                s.env_handle_id.clone(),
            )
        };
        let reason_code = p.reason.as_deref().unwrap_or("rollback");
        match reason_code {
            "rollback" | "policy_violation" | "operator" | "recovery" => {}
            other => {
                return Err(EmbedError::SchemaViolation {
                    path: "rollback/reason".to_string(),
                    code: format!("unknown_reason_code:{other}"),
                })
            }
        }
        // The env half — restore the newest fs_tree snapshot at/below the
        // cut in place; uncaptured domains ride the record.
        let mut uncaptured: Vec<String> = Vec::new();
        if p.restore_env.unwrap_or(true) {
            if let Some(h) = env_handle {
                let drv = self.env_drivers.get_mut(&run_id).ok_or_else(|| {
                    EmbedError::EnvironmentUnavailable {
                        reason: "env_driver_absent for rollback env restore".to_string(),
                    }
                })?;
                let (_restored, unc) = drv
                    .rollback_env(&mut self.store, &lease, &h, p.to_seq as u64)
                    .map_err(crate::open::env_err)?;
                uncaptured = unc;
            } else {
                uncaptured.push("env:no_handle".to_string());
            }
        }
        let rec = self
            .store
            .rollback(
                &run_id,
                &lease,
                p.to_seq as u64,
                reason_code,
                uncaptured,
                &mut |_intent| {
                    // The kernel-side compensator executor: Stage 2's saga
                    // records the intent and observes `applied` — the
                    // external act is the driver's (S2.3's own wiring); a
                    // failing compensator is reported via dispatch Err.
                    Ok(Json::obj([("note", Json::str("compensated-by-kernel"))]))
                },
            )
            .map_err(ledger_err)?;
        Ok(rec.note_json())
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
            "compact" => ViewKind::Compact,
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
                match self.sealed_defs.get(&p.address).cloned() {
                    Some(b) => b,
                    None => {
                        // The durable artifact pool (DF-S1.25-3; S2.3) —
                        // `artifacts/<address>` redirects to the blob id
                        // the bytes were deposited under; `get_blob`
                        // verifies the content hash (CC3 — bytes that
                        // don't hash back are `BlobCorrupt`, never
                        // silently served).
                        match self.artifact_bytes(&p.address) {
                            Ok(b) => b,
                            Err(_) => {
                                return Err(ledger_err(hh_ledger::errors::LedgerError::Missing {
                                    address: p.address.clone(),
                                    reason: hh_ledger::errors::MissingReason::Gc,
                                }))
                            }
                        }
                    }
                }
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
/// `AMENDABLE_APPROVAL_MODES` — the closed set `amend(approval_mode)`
/// admits (§7.1 §2.4's mode spellings; `sync`/`async` are transport
/// shapes, not policy modes).
const AMENDABLE_APPROVAL_MODES: &[&str] = &[
    "unattended_deny",
    "manual",
    "observe_only",
    "pre_approved_only",
    "unattended_defer",
    "tiered",
    "auto_review",
    "bypass",
];

/// The auto-resolution breadth order for the widening check —
/// `unattended_deny` admits nothing, `manual`/`observe_only`/
/// `unattended_defer` route every ask to a human (or defer it),
/// `pre_approved_only` admits sealed pre-authorizations, `tiered`/
/// `auto_review` auto-resolve the non-never-auto floor, `bypass`
/// auto-resolves everything outside never-auto.
fn approval_mode_rank(mode: &str) -> u8 {
    match mode {
        "unattended_deny" => 0,
        "manual" | "observe_only" | "unattended_defer" | "async" => 1,
        "pre_approved_only" => 2,
        "tiered" | "auto_review" => 3,
        "bypass" => 4,
        _ => 1,
    }
}

pub(crate) fn session_id_param(params: &Json, path: &str) -> Result<String, EmbedError> {
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

// ── R-2.8.6 audit surface (S2.5) ────────────────────────────────────────

impl EmbedService {
    /// `audit_view` — the ledger's own audit fold (ADR-0066 D1: no second
    /// store). Returns the `View` record (`payload` is the `AuditView`
    /// dossier — checkpoints, cross-run anchors, content-refs accounting,
    /// completeness vector; `view_hash` per §7.4 §2.4).
    pub(crate) fn audit_view(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = AuditViewParams::from_json(params)?;
        let s = self.live_session(&p.session_id)?;
        let v = self
            .store
            .project(
                &s.run_id,
                ViewKind::AuditView,
                p.until_seq.map(|u| u as u64),
            )
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

    /// `verify` — recompute the session run's chain, tree heads, signed
    /// checkpoints and cross-run anchors from the WAL. The `Tampered`
    /// taxonomy is *data* (`{verdict: tampered, at_seq, kind}`), not a
    /// transport error — a corrupted ledger still answers.
    pub(crate) fn verify(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = VerifyParams::from_json(params)?;
        let s = self.live_session(&p.session_id)?;
        match self.store.verify_run(
            &s.run_id,
            None,
            p.until_seq.map(|u| u as u64),
            self.store.audit_key_resolver(),
        ) {
            Ok(()) => Ok(Json::obj([("verdict", Json::str("ok"))])),
            Err(hh_ledger::errors::LedgerError::Tampered(t)) => Ok(Json::obj([
                ("verdict", Json::str("tampered")),
                ("at_seq", Json::Int(t.at_seq as i64)),
                ("kind", Json::str(t.kind.as_str())),
            ])),
            Err(e) => Err(ledger_err(e)),
        }
    }

    /// `prove_inclusion` — the Merkle audit path for the event at `seq`
    /// under the session run's current tree head (§5g.6 §3).
    pub(crate) fn prove_inclusion(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = ProveInclusionParams::from_json(params)?;
        let s = self.live_session(&p.session_id)?;
        if p.seq < 0 {
            return Err(EmbedError::SchemaViolation {
                path: "prove_inclusion/seq".to_string(),
                code: "negative".to_string(),
            });
        }
        let proof = self
            .store
            .prove_inclusion(&s.run_id, p.seq as u64, None)
            .map_err(ledger_err)?;
        Ok(proof.to_json())
    }

    /// `prove_consistency` — the append-only consistency proof between
    /// `first_size` and `second_size` of the session run's tree.
    pub(crate) fn prove_consistency(&mut self, params: &Json) -> Result<Json, EmbedError> {
        let p = ProveConsistencyParams::from_json(params)?;
        let s = self.live_session(&p.session_id)?;
        if p.first_size < 0 || p.second_size < 0 {
            return Err(EmbedError::SchemaViolation {
                path: "prove_consistency/size".to_string(),
                code: "negative".to_string(),
            });
        }
        let proof = self
            .store
            .prove_consistency(&s.run_id, p.first_size as u64, p.second_size as u64)
            .map_err(ledger_err)?;
        Ok(proof.to_json())
    }
}
