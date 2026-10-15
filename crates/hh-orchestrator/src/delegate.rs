//! `SpawnDelegatePort` — the `hh_control::driver::DelegatePort` impl that
//! executes an admitted `delegate` decision through `hh-subagent`'s one
//! `spawn` path (R-2.6.3; R3.3; CC2 — the decision *asks*, the child
//! lease/grant machinery *confers*: attenuation, budget `≤` parent and the
//! ownership table are the substrate's own checks, never re-declared
//! here). The port binds a [`ParentBinding`] once per parent run; the
//! driver hands it the `control.decision` row's `EventRef` (the
//! idempotency key's decision half) and the decision's owner stamp.
//!
//! `spec` is the `SubagentSpec` the decision carried — parsed through the
//! one canonical codec (`SubagentSpec::from_json`); a malformed ask is
//! `Refused{malformed_subagent_spec}`, never coerced. A model-emitted ask
//! (`control.delegate.proposed`) names the staged record by content
//! address — `spec{spec_ref: "sha256:<digest>"}` — the wire-bound ask
//! (`model.call.completed`'s audit bound) resolved through the seam's
//! blob store; an unresolvable ref is `Refused{unresolvable_spec_ref}`,
//! never a guessed shape. `budget_slice` / `permissions` are the
//! envelope's audit members — the spec's own `budget`/`requested_grants`
//! are what `spawn` enforces (SP-1 — the ask and the authority record
//! cannot diverge by construction).
//!
//! `SpawnError::Refused` returns its `SpawnRefused` class spelling — a
//! typed observation β owns (the run continues); `Kernel` is the
//! substrate's typed failure, surfaced verbatim. Both land on the parent
//! run's fenced writer through `StoreSeam` — durable-before-visible (CC3).

use hh_control::driver::{DelegateOutcome, DelegatePort, StoreSeam};
use hh_ledger::manifest::EventRef;
use hh_monitor::table::HandleTable;
use hh_ontology::control::Owner;
use hh_wire::json::Json;

use hh_subagent::spawn::{EnvDeriver, SpawnCtx};
use hh_subagent::types::{SpawnError, SubagentSpec};

use crate::orchestrator::ParentBinding;

/// The spawn-backed `DelegatePort` — one binding per parent run.
pub struct SpawnDelegatePort {
    /// The parent seam set (lease/handle/budget/ownership/depth/coords).
    pub binding: ParentBinding,
    /// The parent's live authority table — Π's `delegate` input.
    pub handles: HandleTable,
    /// The derive port for `isolation.environment = derive{…}` — `None`
    /// refuses `Derive` requests `ModeUnsupported`.
    pub env_driver: Option<Box<dyn EnvDeriver>>,
}

impl DelegatePort for SpawnDelegatePort {
    fn delegate(
        &mut self,
        seam: StoreSeam<'_>,
        decision_ref: &EventRef,
        owner: Owner,
        spec: &Json,
        _budget_slice: &Json,
        _permissions: &Json,
    ) -> DelegateOutcome {
        // Resolve the ask — a `spec_ref` member names the staged record
        // by content address (the model-emitted leg's bounded wire form);
        // otherwise `spec` IS the record (the compiled-plan leg).
        let spec_json = match spec.get("spec_ref").and_then(Json::as_str) {
            Some(addr) => {
                let Some(digest) = addr.strip_prefix("sha256:") else {
                    return DelegateOutcome::Refused {
                        class: "malformed_subagent_spec".into(),
                        detail: format!("spec_ref {addr} is not a sha256 address"),
                    };
                };
                let ca = hh_identity::idp::ContentAddress {
                    idp: hh_identity::idp::IDP_1.idp_id,
                    algorithm: "sha256",
                    digest: digest.to_string(),
                    media_type: "application/json".into(),
                    size: 0,
                };
                match seam.store.get_blob(&ca) {
                    Ok(bytes) => match std::str::from_utf8(&bytes)
                        .ok()
                        .and_then(|t| hh_wire::json::parse(t).ok())
                    {
                        Some(j) => j,
                        None => {
                            return DelegateOutcome::Refused {
                                class: "malformed_subagent_spec".into(),
                                detail: format!("spec_ref {addr} is not canonical json"),
                            }
                        }
                    },
                    Err(e) => {
                        return DelegateOutcome::Refused {
                            class: "unresolvable_spec_ref".into(),
                            detail: format!("{addr}: {e:?}"),
                        }
                    }
                }
            }
            None => spec.clone(),
        };
        let Some(spec) = SubagentSpec::from_json(&spec_json) else {
            return DelegateOutcome::Refused {
                class: "malformed_subagent_spec".into(),
                detail: "spec fails the closed SubagentSpec codec".into(),
            };
        };
        // SP-1 — a declared ask-level `delegation_reason` and the spec's
        // own cannot diverge: the audit shows one reason, the authority
        // record confers the same, or the ask refuses.
        if let Some(asked) = spec_json.get("delegation_reason").and_then(Json::as_str) {
            if asked != spec.delegation_reason.as_str() {
                return DelegateOutcome::Refused {
                    class: "delegation_reason_mismatch".into(),
                    detail: format!("ask {asked} vs spec {}", spec.delegation_reason.as_str()),
                };
            }
        }
        let b = &self.binding;
        let mut ctx = SpawnCtx {
            store: seam.store,
            handles: &self.handles,
            parent_run_id: &b.parent_run_id,
            parent_lease: &b.parent_lease,
            parent_handle_id: &b.parent_handle_id,
            parent_budget_id: &b.parent_budget_id,
            parent_env_id: b.parent_env_id.as_deref(),
            env_driver: self.env_driver.as_deref_mut(),
            parent_ownerships: &b.ownerships,
            parent_tool_table: &b.tool_table,
            parent_depth: b.parent_depth,
            decision: decision_ref.clone(),
            owner,
            holder: &b.holder,
            coords: b.coords.clone(),
            reserve_ttl_ms: b.reserve_ttl_ms,
            branch_ctx: None,
            hosted_plane: None,
            hook: None,
        };
        match hh_subagent::spawn::spawn(&mut ctx, &spec) {
            Ok(s) => DelegateOutcome::Spawned {
                child_run_id: s.child_run_id,
            },
            Err(SpawnError::Refused(r)) => DelegateOutcome::Refused {
                class: r.as_str().into(),
                detail: r.to_json().to_canonical_string(),
            },
            Err(SpawnError::Kernel(e)) => DelegateOutcome::Refused {
                class: "kernel_error".into(),
                detail: e,
            },
        }
    }
}
