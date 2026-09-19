//! The kernel event builders — `action.environment.*` (the §5a.5 §3 lifecycle
//! family, kernel-origin) and the `action.effect.*`/`action.tool.*` members the
//! seven-stage dispatcher emits (§5d.5). Every builder mints an `Event` with
//! `Producer::kernel` + `ProvenanceRecord::kernel` (kernel-origin rows are
//! audit-recorded — the class table's `kernel_origin`/`requires_provenance`
//! flags read them).

use std::collections::BTreeMap;

use hh_ledger::event::{Event, Producer, Scope};
use hh_ledger::manifest::EventRef;
use hh_ledger::store::Store;
use hh_ledger::LedgerError;
use hh_provenance::ProvenanceRecord;
use hh_wire::json::Json;

use crate::handle::EnvHandle;

/// The emitting component label (`producer.component_variant_ref`).
pub const COMPONENT: &str = "hh-env";

/// `EventMinter` — the kernel event factory (the `hh-budget` `Account::mint`
/// pattern: `alloc_id` + `ts_now` + `head_event_id` + `ProvenanceRecord::
/// kernel`). Borrows the `Store`; `append` is the caller's.
pub struct EventMinter<'a> {
    store: &'a Store,
    run_id: &'a str,
}

impl<'a> EventMinter<'a> {
    /// A minter over `store` for `run_id`.
    pub fn new(store: &'a Store, run_id: &'a str) -> Self {
        EventMinter { store, run_id }
    }

    /// Mint a kernel event (`Producer::kernel` + kernel provenance; the
    /// caller sets `scope`, `causes`, `refs`, `ir_refs` via `with_*`).
    pub fn mint(&self, class: &str, payload: Json) -> Result<Event, LedgerError> {
        self.build(class, payload, self.store.alloc_id("evt"))
    }

    /// Mint under a caller-allocated `event_id` — the handle-mint path's
    /// form: a `security.permission.granted` row's envelope id *is* the
    /// handle's `issued_at` and the fold's `holder` pin
    /// (`handle_from_granted` re-pins `holder.version = Pinned(event_id)`),
    /// so the id is allocated with the handle — never re-rolled at emission.
    pub fn mint_with_id(
        &self,
        class: &str,
        payload: Json,
        event_id: String,
    ) -> Result<Event, LedgerError> {
        self.build(class, payload, event_id)
    }

    fn build(&self, class: &str, payload: Json, event_id: String) -> Result<Event, LedgerError> {
        Ok(Event {
            event_id,
            class: class.to_string(),
            ts: self.store.ts_now(),
            hlc: None,
            producer: Producer::kernel(COMPONENT),
            scope: Scope::default(),
            parent_event_id: self.store.head_event_id(self.run_id)?,
            causes: vec![],
            refs: vec![],
            ir_refs: vec![],
            surface_ids: BTreeMap::new(),
            provenance: Some(ProvenanceRecord::kernel(COMPONENT, self.store.now_ms())),
            content_kind: None,
            payload,
        })
    }

    /// Mint with an effect scope (`scope.effect_id` + the chain).
    pub fn mint_effect(
        &self,
        class: &str,
        payload: Json,
        effect_id: &str,
        chain: &ScopeChain,
    ) -> Result<Event, LedgerError> {
        let mut ev = self.mint(class, payload)?;
        // An empty member is `None`, never `Some("")` — `""` is not a scope
        // id (a detached child's fold may legitimately lack a tool_call).
        let nonempty = |s: &str| (!s.is_empty()).then(|| s.to_string());
        ev.scope = Scope {
            turn_id: nonempty(&chain.turn_id),
            model_call_id: nonempty(&chain.model_call_id),
            tool_call_id: nonempty(&chain.tool_call_id),
            effect_id: Some(effect_id.to_string()),
            ..Scope::default()
        };
        Ok(ev)
    }
}

/// `ScopeChain` — the `run ⊃ turn ⊃ model_call ⊃ tool_call` identity the
/// scope members carry (the dispatcher's per-call context).
#[derive(Debug, Clone, PartialEq)]
pub struct ScopeChain {
    /// The turn.
    pub turn_id: String,
    /// The model call.
    pub model_call_id: String,
    /// The tool call.
    pub tool_call_id: String,
}

// ── environment lifecycle ────────────────────────────────────────────────────

/// `action.environment.declared{env_handle, environment_ref, class, image}`.
pub fn declared_payload(h: &EnvHandle) -> Json {
    Json::obj([
        ("env_handle", Json::str(h.env_handle_id.clone())),
        (
            "environment_ref",
            Json::obj([
                ("semantic_id", Json::str(h.environment_ref.0.clone())),
                ("version_id", Json::str(h.environment_ref.1.clone())),
            ]),
        ),
        ("class", Json::str(h.class.as_str())),
        ("image", h.image.to_json()),
    ])
}

/// `action.environment.provisioning{env_handle}`.
pub fn provisioning_payload(h: &EnvHandle) -> Json {
    Json::obj([("env_handle", Json::str(h.env_handle_id.clone()))])
}

/// `action.environment.provisioned{env_handle, image, isolation_class}` — the
/// *resolved* image + the class's implied isolation (pre-attach; the report
/// refines it).
pub fn provisioned_payload(h: &EnvHandle) -> Json {
    Json::obj([
        ("env_handle", Json::str(h.env_handle_id.clone())),
        ("image", h.image.to_json()),
        (
            "isolation_class",
            Json::str(h.class.implied_isolation().as_str()),
        ),
    ])
}

/// `action.environment.attached{env_handle, containment_report_ref,
/// isolation_class}` — `containment_report_ref` names the
/// `security.containment.applied` event the `attach()` produced (the
/// DF-S1.12-3 link).
pub fn attached_payload(h: &EnvHandle, applied_event_ref: &str) -> Json {
    Json::obj([
        ("env_handle", Json::str(h.env_handle_id.clone())),
        ("containment_report_ref", Json::str(applied_event_ref)),
        ("isolation_class", Json::str(h.isolation_class().as_str())),
    ])
}

/// `action.environment.ready{env_handle, isolation_class}`.
pub fn ready_payload(h: &EnvHandle) -> Json {
    Json::obj([
        ("env_handle", Json::str(h.env_handle_id.clone())),
        ("isolation_class", Json::str(h.isolation_class().as_str())),
    ])
}

/// `action.environment.detached{env_handle, reason}`.
pub fn detached_payload(h: &EnvHandle, reason: &str) -> Json {
    Json::obj([
        ("env_handle", Json::str(h.env_handle_id.clone())),
        ("reason", Json::str(reason)),
    ])
}

/// `action.environment.unreachable{env_handle, last_contact_ms}` — contact
/// lost; kernel death is *not* environment death (the heal path decides).
pub fn unreachable_payload(h: &EnvHandle) -> Json {
    Json::obj([
        ("env_handle", Json::str(h.env_handle_id.clone())),
        ("last_contact_ms", Json::Int(h.meters.last_contact() as i64)),
    ])
}

/// `action.environment.reattached{env_handle, heal_count}`.
pub fn reattached_payload(h: &EnvHandle) -> Json {
    Json::obj([
        ("env_handle", Json::str(h.env_handle_id.clone())),
        ("heal_count", Json::Int(h.heal_count as i64)),
    ])
}

/// `action.environment.replaced{env_handle, successor}` — the heal ladder's
/// replace rung: the old handle is terminal, the successor owns the
/// environment (`replaced` names the new handle).
pub fn replaced_payload(h: &EnvHandle, successor: &str) -> Json {
    Json::obj([
        ("env_handle", Json::str(h.env_handle_id.clone())),
        ("successor", Json::str(successor)),
    ])
}

/// `action.environment.derived{env_handle, parent, mode, on_parent_end}` —
/// the `derive` op's record (the child's `ParentEdge` is the handle-side
/// member; this row is the audit).
pub fn derived_payload(h: &EnvHandle) -> Json {
    let mut m = vec![("env_handle", Json::str(h.env_handle_id.clone()))];
    if let Some(p) = &h.parent {
        m.push(("parent", Json::str(p.env_handle_id.clone())));
        m.push((
            "mode",
            Json::str(match p.mode {
                crate::handle::DeriveMode::FreshFromImage => "fresh_from_image",
                crate::handle::DeriveMode::ForkSnapshot => "fork_snapshot",
                crate::handle::DeriveMode::Share => "share",
                crate::handle::DeriveMode::ScopedSubtree => "scoped_subtree",
            }),
        ));
        m.push((
            "on_parent_end",
            Json::str(match p.on_parent_end {
                crate::handle::OnParentEnd::Teardown => "teardown",
                crate::handle::OnParentEnd::DetachToChild => "detach_to_child",
            }),
        ));
    }
    Json::Obj(m.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// `action.environment.torn_down{env_handle, reason}`.
pub fn torn_down_payload(h: &EnvHandle, reason: &str) -> Json {
    Json::obj([
        ("env_handle", Json::str(h.env_handle_id.clone())),
        ("reason", Json::str(reason)),
    ])
}

/// `action.environment.failed{env_handle, reason}`.
pub fn failed_payload(h: &EnvHandle, reason: &str) -> Json {
    Json::obj([
        ("env_handle", Json::str(h.env_handle_id.clone())),
        ("reason", Json::str(reason)),
    ])
}

/// `action.environment.verified{env_handle, report_fresh}` — the explicit
/// verification point (heal/reattach; per-use checks are pure).
pub fn verified_payload(h: &EnvHandle, report_fresh: bool) -> Json {
    Json::obj([
        ("env_handle", Json::str(h.env_handle_id.clone())),
        ("report_fresh", Json::Bool(report_fresh)),
        ("isolation_class", Json::str(h.isolation_class().as_str())),
    ])
}

/// `action.environment.meters_sampled{env_handle, reserved_ms, active_ms,
/// suspended_ms}` — the three kernel clocks (invariant `reserved ≥ active +
/// suspended` holds by construction).
pub fn meters_sampled_payload(h: &EnvHandle, now: u64) -> Json {
    let v = h.meters.view(now);
    Json::obj([
        ("env_handle", Json::str(h.env_handle_id.clone())),
        ("reserved_ms", Json::Int(v.reserved_ms as i64)),
        ("active_ms", Json::Int(v.active_ms as i64)),
        ("suspended_ms", Json::Int(v.suspended_ms as i64)),
    ])
}

/// `action.environment.snapshot{env_handle, snapshot_ref, kind}`.
pub fn snapshot_payload(h: &EnvHandle, snapshot_ref: &str, kind: &str) -> Json {
    Json::obj([
        ("env_handle", Json::str(h.env_handle_id.clone())),
        ("snapshot_ref", Json::str(snapshot_ref)),
        ("kind", Json::str(kind)),
    ])
}

// ── effect lifecycle (the dispatcher's seven stages) ─────────────────────────

/// `action.effect.intended{effective_risk_class, declared_risk_class?,
/// capability_version, args_canonical_hash, ordinal, parent_effect_id?}` —
/// opens the effect scope.
pub fn intended_payload(
    effective: &hh_ontology::risk::RiskClass,
    declared: Option<&hh_ontology::risk::RiskClass>,
    capability_version: &str,
    args_canonical_hash: &str,
    ordinal: u64,
) -> Json {
    let mut m = BTreeMap::new();
    m.insert("effective_risk_class".to_string(), effective.to_json());
    if let Some(d) = declared {
        m.insert("declared_risk_class".to_string(), d.to_json());
    }
    m.insert(
        "capability_version".to_string(),
        Json::str(capability_version),
    );
    m.insert(
        "args_canonical_hash".to_string(),
        Json::str(args_canonical_hash),
    );
    m.insert("ordinal".to_string(), Json::Int(ordinal as i64));
    Json::Obj(m)
}

/// `action.effect.authorized{effective_risk_class?}` — the decision was
/// `allow` (the `security.permission.decided` row is the gate's record; this
/// is the effect-side mirror — raise-only on the class).
pub fn authorized_payload(effective: &hh_ontology::risk::RiskClass) -> Json {
    Json::obj([("effective_risk_class", effective.to_json())])
}

/// `action.effect.prepared{idempotency_key, baseline_ref?,
/// compensation_plan_id?, attribution_token_hash, deadline, output_policy_ref}`
/// — the write-ahead record's prepare half.
#[allow(clippy::too_many_arguments)]
pub fn prepared_payload(
    idempotency_key: &str,
    baseline_ref: Option<&str>,
    compensation_plan_id: Option<&str>,
    attribution_token_hash: &str,
    deadline: Option<u64>,
    output_policy_ref: &str,
) -> Json {
    let mut m = BTreeMap::new();
    m.insert("idempotency_key".to_string(), Json::str(idempotency_key));
    if let Some(b) = baseline_ref {
        m.insert("baseline_ref".to_string(), Json::str(b));
    }
    if let Some(c) = compensation_plan_id {
        m.insert("compensation_plan_id".to_string(), Json::str(c));
    }
    m.insert(
        "attribution_token_hash".to_string(),
        Json::str(attribution_token_hash),
    );
    m.insert(
        "deadline".to_string(),
        deadline.map_or(Json::Null, |d| Json::Int(d as i64)),
    );
    m.insert(
        "output_policy_ref".to_string(),
        Json::str(output_policy_ref),
    );
    Json::Obj(m)
}

/// `action.effect.committed{attempt_no, fencing_token, dispatched_at}` — the
/// write-ahead record is durable; dispatch may proceed.
pub fn committed_payload(attempt_no: u64, fencing_token: u64, dispatched_at: u64) -> Json {
    Json::obj([
        ("attempt_no", Json::Int(attempt_no as i64)),
        ("fencing_token", Json::Int(fencing_token as i64)),
        ("dispatched_at", Json::Int(dispatched_at as i64)),
    ])
}

/// `action.effect.observed{attempt_no, fencing_token, outcome, status,
/// exit_status?, capture_manifest_ref, completeness}`.
pub fn observed_payload(
    attempt_no: u64,
    fencing_token: u64,
    obs: &crate::observe::Observation,
) -> Json {
    let mut m = BTreeMap::new();
    m.insert("attempt_no".to_string(), Json::Int(attempt_no as i64));
    m.insert("fencing_token".to_string(), Json::Int(fencing_token as i64));
    m.insert("outcome".to_string(), Json::str(obs.outcome.as_str()));
    let obsj = obs.to_json();
    if let Json::Obj(om) = obsj {
        // flatten the observation's members into the payload (status/error,
        // manifest_ref, completeness).
        for (k, v) in om {
            m.insert(k, v);
        }
    }
    Json::Obj(m)
}

/// `context.observation.recorded{kind: "tool_observation", effect_id,
/// capability_ref, capture_manifest_ref, outcome, admission, label}` — the
/// observation-plane record of a `flow_contract` capability's admitted
/// result (§5g.2 §3's payload extension; §5d.5 observe's second output).
/// `kind` discriminates the reminder rows (`budget_reminder`) the class
/// also carries; `admission`/`label` are the recorded admission kind and
/// the admitted `L(r)` — content-free (ids, enums, the label triple).
pub fn observation_recorded_payload(
    effect_id: &str,
    capability_semantic_id: &str,
    manifest_ref: &str,
    outcome: &crate::observe::EffectOutcome,
    admission: &hh_provenance::flow::Admission,
) -> Json {
    Json::obj([
        ("kind", Json::str("tool_observation")),
        ("effect_id", Json::str(effect_id.to_string())),
        (
            "capability_ref",
            Json::str(capability_semantic_id.to_string()),
        ),
        ("capture_manifest_ref", Json::str(manifest_ref.to_string())),
        ("outcome", Json::str(outcome.as_str())),
        ("admission", Json::str(admission.kind.as_str())),
        (
            "label",
            hh_provenance::flow::label_json_full(&admission.label),
        ),
    ])
}

/// `action.effect.unknown{attempt_no?, fencing_token, cause}` — outcome
/// unknowable; must be probed, never silently redispatched.
pub fn unknown_payload(attempt_no: u64, fencing_token: u64, cause: &str) -> Json {
    Json::obj([
        ("attempt_no", Json::Int(attempt_no as i64)),
        ("fencing_token", Json::Int(fencing_token as i64)),
        ("cause", Json::str(cause)),
    ])
}

/// `action.effect.probed{fencing_token, verdict}` — the probe's answer.
pub fn probed_payload(fencing_token: u64, verdict: &str) -> Json {
    Json::obj([
        ("fencing_token", Json::Int(fencing_token as i64)),
        ("verdict", Json::str(verdict)),
    ])
}

/// `action.effect.refused{reason}` — the monitor's deny (terminal).
pub fn refused_payload(reason: &str) -> Json {
    Json::obj([("reason", Json::str(reason))])
}

/// `action.effect.unattributed{signal_kind, evidence_ref, detection}` — the
/// scope-free marker (a capture-path signal that resolved to no effect).
pub fn unattributed_payload(signal_kind: &str, evidence_ref: &str, detection: &str) -> Json {
    Json::obj([
        ("signal_kind", Json::str(signal_kind)),
        ("evidence_ref", Json::str(evidence_ref)),
        ("detection", Json::str(detection)),
    ])
}

// ── tool call rows ───────────────────────────────────────────────────────────

/// `action.tool.proposed{capability_ref}` — opens the tool_call scope.
pub fn tool_proposed_payload(capability_semantic_id: &str, capability_version: &str) -> Json {
    Json::obj([(
        "capability_ref",
        Json::obj([
            ("semantic_id", Json::str(capability_semantic_id)),
            ("version_id", Json::str(capability_version)),
        ]),
    )])
}

/// `action.tool.started{execution_id, attribution_token_hash}` (CF-214 — the
/// audit-grade dispatch row).
pub fn tool_started_payload(execution_id: &str, attribution_token_hash: &str) -> Json {
    Json::obj([
        ("execution_id", Json::str(execution_id)),
        ("attribution_token_hash", Json::str(attribution_token_hash)),
    ])
}

/// `action.tool.completed{status}` — the tool's terminal (`ok|error`).
pub fn tool_completed_payload(status: &str, detail: Option<&str>) -> Json {
    let mut m = BTreeMap::new();
    m.insert("status".to_string(), Json::str(status));
    if let Some(d) = detail {
        m.insert("detail".to_string(), Json::str(d));
    }
    Json::Obj(m)
}

/// `action.tool.rejected{source}` — a protocol/validation refusal.
pub fn tool_rejected_payload(source: &str, reason: &str) -> Json {
    Json::obj([("source", Json::str(source)), ("reason", Json::str(reason))])
}

/// The `security.containment.applied` ref the `attached` event's
/// `containment_report_ref` names — an `EventRef` the caller resolves.
pub fn applied_event_ref(run_id: &str, event_id: &str) -> EventRef {
    EventRef {
        run_id: run_id.to_string(),
        event_id: event_id.to_string(),
    }
}
