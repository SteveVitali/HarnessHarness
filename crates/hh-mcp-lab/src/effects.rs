//! The surface-session effect chain (spec §7.3 §2.5): every `tools/call`
//! runs `prepared → dispatched → observed` under kernel-held authority —
//! `action.effect.intended` (the dossier verbatim: `capability`,
//! `args_canonical_hash`, declared risk, budget node, expiry, actor),
//! `action.effect.authorized` (the Π allow — the binding's own
//! authority decides), `action.effect.prepared` (the *derived*
//! idempotency key — `H(run, effect, args, capability_version)`, never a
//! caller-supplied token), then the write-ahead `committed` for
//! mutating calls (which requires the durable
//! `security.permission.decided{allow}` — complete mediation, I-H7) or
//! the direct `observed` for `read_only` calls.
//!
//! Refusals are `action.effect.refused{reason}` — the typed refusal
//! never becomes a protocol fault and the durable record shows the
//! decision.

use hh_embed::service::EmbedService;
use hh_embed_schema::errors::EmbedError;
use hh_ledger::event::{Event, Scope};
use hh_ledger::manifest::EventRef;
use hh_wire::json::Json;

use crate::binding::CallerBinding;
use crate::exposure::EffectDecl;
use crate::session::{mint_event, SurfaceSession};

/// One call's effect context — minted at `tools/call`, threaded through
/// the chain.
pub struct EffectCtx {
    /// The minted effect id (`ef-<n>`).
    pub effect_id: String,
    /// `H(canonical arguments)` — the dossier + idempotency preimage.
    pub args_hash: String,
    /// The dossier's `capability_version` — the tool's semantic id pins
    /// the served version (the exposure def's `version_id` narrows it).
    pub capability_version: String,
    /// The derived idempotency key (`prepared` requires it).
    pub idempotency_key: String,
    /// The tool's declared risk/effect shape.
    pub decl: EffectDecl,
    /// The `intended` row's event ref — the launch causality anchor
    /// (`spawn_event`) and every later row's `causes[]` head.
    pub intended_ref: Option<EventRef>,
    /// The `prepared`/`committed` refs, as they land.
    pub prepared_ref: Option<EventRef>,
    /// The `committed` ref.
    pub committed_ref: Option<EventRef>,
    /// The terminal row's ref (`observed`/`refused`).
    pub terminal_ref: Option<EventRef>,
}

impl EffectCtx {
    /// Mint the context — pure id derivation; nothing durable yet.
    pub fn mint(
        svc: &mut EmbedService,
        surface_run_id: &str,
        tool_semantic_id: &str,
        exposure_version_id: &str,
        arguments: &Json,
        decl: &EffectDecl,
    ) -> EffectCtx {
        let effect_id = svc.surface_alloc_id("ef");
        let args_hash =
            hh_identity::idp_id("mcp.call_args", arguments.to_canonical_string().as_bytes());
        let capability_version = format!("{tool_semantic_id}@{exposure_version_id}");
        let idempotency_key = hh_ledger::effect::idempotency_key(
            surface_run_id,
            &effect_id,
            &args_hash,
            &capability_version,
        );
        EffectCtx {
            effect_id,
            args_hash,
            capability_version,
            idempotency_key,
            decl: decl.clone(),
            intended_ref: None,
            prepared_ref: None,
            committed_ref: None,
            terminal_ref: None,
        }
    }

    fn scope(&self, turn_id: &str) -> Scope {
        Scope {
            turn_id: Some(turn_id.to_string()),
            effect_id: Some(self.effect_id.clone()),
            ..Scope::default()
        }
    }

    /// `action.effect.intended` — the declared audit members of the
    /// dossier (capability, both risk classes, the derived
    /// idempotency key; the argument bytes stay off-ledger — I7).
    /// Returns the row's `EventRef` — the launch path's `spawn_event`.
    pub fn intended(
        &mut self,
        svc: &mut EmbedService,
        session: &SurfaceSession,
        turn_id: &str,
        _binding: &CallerBinding,
        tool_name: &str,
        _arguments: &Json,
    ) -> Result<EventRef, EmbedError> {
        // Rule C (§5g.6 I-A1): `action.effect.intended` is audit-grade —
        // only declared `audit_fields` ride the payload. The dossier's
        // bounded members land individually (`capability`,
        // `capability_version`, `args_canonical_hash`, both risk
        // classes, the derived `idempotency_key`); the caller-binding
        // coordinates are already durable on `decided{decider,
        // decider_ref}` and the envelope's provenance, and `arguments`
        // verbatim is content — never an audit member (I7).
        let ev = mint_event(
            svc,
            &session.run_id,
            "action.effect.intended",
            Json::obj([
                ("effect_id", Json::str(self.effect_id.clone())),
                ("capability", Json::str(tool_name)),
                (
                    "capability_version",
                    Json::str(self.capability_version.clone()),
                ),
                ("args_canonical_hash", Json::str(self.args_hash.clone())),
                ("declared_risk_class", self.decl.risk_class.clone()),
                ("effective_risk_class", self.decl.risk_class.clone()),
                ("idempotency_key", Json::str(self.idempotency_key.clone())),
            ]),
            self.scope(turn_id),
            vec![],
        )?;
        let r = svc.surface_append(&session.run_id, &session.lease, vec![ev])?;
        let event_id = session.events(svc)?[r.first as usize].event_id.clone();
        let eref = EventRef {
            run_id: session.run_id.clone(),
            event_id,
        };
        self.intended_ref = Some(eref.clone());
        Ok(eref)
    }

    /// `security.permission.decided{decision}` + `action.effect.
    /// authorized` — the durable decision precedes `prepared` (AC-R-
    /// 2.8.7-1); `committed` needs the `decided{allow}` specifically
    /// (I-H7). For a `deny` the caller then lands `refused`, never
    /// `prepared`.
    pub fn decide_and_authorize(
        &mut self,
        svc: &mut EmbedService,
        session: &SurfaceSession,
        turn_id: &str,
        binding: &CallerBinding,
        decision: &str,
        reason: &str,
    ) -> Result<(), EmbedError> {
        let decided = mint_event(
            svc,
            &session.run_id,
            "security.permission.decided",
            Json::obj([
                ("effect_id", Json::str(self.effect_id.clone())),
                ("attempt_no", Json::Int(1)),
                ("decision", Json::str(decision)),
                ("decider", Json::str(binding.binding_id.clone())),
                ("decider_ref", Json::str(binding.principal_ref.clone())),
                ("reason", Json::str(reason)),
                ("effective_risk_class", self.decl.risk_class.clone()),
                ("effective_authority", binding.authority_cap.clone()),
            ]),
            self.scope(turn_id),
            vec![],
        )?;
        let authorized = mint_event(
            svc,
            &session.run_id,
            "action.effect.authorized",
            Json::obj([
                ("effect_id", Json::str(self.effect_id.clone())),
                ("decision_ref", Json::str(binding.binding_id.clone())),
                ("effective_risk_class", self.decl.risk_class.clone()),
            ]),
            self.scope(turn_id),
            vec![],
        )?;
        svc.surface_append(&session.run_id, &session.lease, vec![decided, authorized])?;
        Ok(())
    }

    /// `action.effect.authorized` alone — the Π allow for a `read_only`
    /// call (no `decided` owed: `committed` never lands).
    pub fn authorize(
        &mut self,
        svc: &mut EmbedService,
        session: &SurfaceSession,
        turn_id: &str,
    ) -> Result<(), EmbedError> {
        let ev = mint_event(
            svc,
            &session.run_id,
            "action.effect.authorized",
            Json::obj([
                ("effect_id", Json::str(self.effect_id.clone())),
                ("effective_risk_class", self.decl.risk_class.clone()),
            ]),
            self.scope(turn_id),
            vec![],
        )?;
        svc.surface_append(&session.run_id, &session.lease, vec![ev])?;
        Ok(())
    }

    /// `action.effect.prepared{idempotency_key}` — the derived key
    /// (AC-R-2.2.2-7's equality check runs in the append gate).
    pub fn prepared(
        &mut self,
        svc: &mut EmbedService,
        session: &SurfaceSession,
        turn_id: &str,
    ) -> Result<EventRef, EmbedError> {
        let mut members: Vec<(String, Json)> = vec![
            ("effect_id".to_string(), Json::str(self.effect_id.clone())),
            (
                "idempotency_key".to_string(),
                Json::str(self.idempotency_key.clone()),
            ),
        ];
        if self.decl.compensable {
            members.push((
                "compensation_plan_id".to_string(),
                Json::str(format!("plan-{}", self.effect_id)),
            ));
        } else if !self.decl.read_only {
            members.push(("baseline_ref".to_string(), Json::str("baseline:surface")));
        }
        let ev = mint_event(
            svc,
            &session.run_id,
            "action.effect.prepared",
            Json::Obj(members.into_iter().collect()),
            self.scope(turn_id),
            vec![],
        )?;
        let r = svc.surface_append(&session.run_id, &session.lease, vec![ev])?;
        let event_id = session.events(svc)?[r.first as usize].event_id.clone();
        let eref = EventRef {
            run_id: session.run_id.clone(),
            event_id,
        };
        self.prepared_ref = Some(eref.clone());
        Ok(eref)
    }

    /// `action.effect.committed{attempt_no, fencing_token}` — the
    /// write-ahead record (needs the `decided{allow}`).
    pub fn committed(
        &mut self,
        svc: &mut EmbedService,
        session: &SurfaceSession,
        turn_id: &str,
    ) -> Result<EventRef, EmbedError> {
        let ev = mint_event(
            svc,
            &session.run_id,
            "action.effect.committed",
            Json::obj([
                ("effect_id", Json::str(self.effect_id.clone())),
                ("attempt_no", Json::Int(1)),
                ("fencing_token", Json::Int(session.lease.generation as i64)),
            ]),
            self.scope(turn_id),
            vec![],
        )?;
        let r = svc.surface_append(&session.run_id, &session.lease, vec![ev])?;
        let event_id = session.events(svc)?[r.first as usize].event_id.clone();
        let eref = EventRef {
            run_id: session.run_id.clone(),
            event_id,
        };
        self.committed_ref = Some(eref.clone());
        Ok(eref)
    }

    /// `action.effect.observed{attempt_no, outcome, fencing_token}` —
    /// the terminal row the op's result rides.
    pub fn observed(
        &mut self,
        svc: &mut EmbedService,
        session: &SurfaceSession,
        turn_id: &str,
        outcome: &str,
        result_ref: Option<&Json>,
    ) -> Result<EventRef, EmbedError> {
        let mut m: Vec<(String, Json)> = vec![
            ("effect_id".to_string(), Json::str(self.effect_id.clone())),
            ("attempt_no".to_string(), Json::Int(1)),
            ("outcome".to_string(), Json::str(outcome)),
            (
                "fencing_token".to_string(),
                Json::Int(session.lease.generation as i64),
            ),
        ];
        if let Some(r) = result_ref {
            // `outcome_ref` is the declared member — the result's
            // canonical bytes are content, so the row carries their
            // `idp/1` digest (a ref, not the bytes — I7; the bound is
            // 512B).
            m.push((
                "outcome_ref".to_string(),
                Json::str(hh_identity::idp_id(
                    "mcp.call_result",
                    r.to_canonical_string().as_bytes(),
                )),
            ));
        }
        let ev = mint_event(
            svc,
            &session.run_id,
            "action.effect.observed",
            Json::Obj(m.into_iter().collect()),
            self.scope(turn_id),
            vec![],
        )?;
        let r = svc.surface_append(&session.run_id, &session.lease, vec![ev])?;
        let event_id = session.events(svc)?[r.first as usize].event_id.clone();
        let eref = EventRef {
            run_id: session.run_id.clone(),
            event_id,
        };
        self.terminal_ref = Some(eref.clone());
        Ok(eref)
    }

    /// `action.effect.refused{reason}` — the typed refusal's durable
    /// row (from `intended`/`authorized`).
    pub fn refused(
        &mut self,
        svc: &mut EmbedService,
        session: &SurfaceSession,
        turn_id: &str,
        reason: &str,
        detail: Json,
    ) -> Result<EventRef, EmbedError> {
        // `error` is the declared bounded member (512B) — the typed
        // `reason` names the refusal; the detail's canonical form rides
        // verbatim when it fits, else as its `idp/1` digest (still the
        // complete record — never a silent truncation).
        let detail_str = detail.to_canonical_string();
        let error = if detail_str.len() <= 500 {
            detail_str
        } else {
            hh_identity::idp_id("mcp.refusal_detail", detail_str.as_bytes())
        };
        let ev = mint_event(
            svc,
            &session.run_id,
            "action.effect.refused",
            Json::obj([
                ("effect_id", Json::str(self.effect_id.clone())),
                ("reason", Json::str(reason)),
                ("error", Json::str(error)),
            ]),
            self.scope(turn_id),
            vec![],
        )?;
        let r = svc.surface_append(&session.run_id, &session.lease, vec![ev])?;
        let event_id = session.events(svc)?[r.first as usize].event_id.clone();
        let eref = EventRef {
            run_id: session.run_id.clone(),
            event_id,
        };
        self.terminal_ref = Some(eref.clone());
        Ok(eref)
    }
}

/// A batch of caller-built `Event`s → one `surface_append` (the
/// batch-local fold lets `authorized`/`decided` precede `prepared` in
/// the same append — the §5a.2 batch semantics).
pub fn append_batch(
    svc: &mut EmbedService,
    session: &SurfaceSession,
    events: Vec<Event>,
) -> Result<hh_ledger::event::SeqRange, EmbedError> {
    svc.surface_append(&session.run_id, &session.lease, events)
}
