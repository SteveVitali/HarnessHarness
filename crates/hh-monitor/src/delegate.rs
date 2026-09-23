//! `delegate(parent, child_spec, requested, budget_req, at)` — the only way a
//! child (subagent, hosted participant, evolution service) obtains authority
//! (§5g.1 §2.1; ADR-0053 D3 D-1/D-2; step (3) of the one `spawn`).
//!
//! Downward-only attenuation (I-H4): every requested `Grant` must be covered by
//! a `delegable` grant of a live parent — same domain, the parent's
//! `EffectClass` covering the child's, the child's scope ⊆ the parent's under
//! the interim matcher, and the child's constraints tightening (count/time ≤
//! the parent's). `ceiling = min(requested, parent.ceiling)`; `environment`
//! mints never carry `fs_write`/`exec`/`secret_access` (§2.5 row 1). Requests
//! are attenuated to what is coverable, never rounded up — a request the parent
//! cannot cover is `AuthorityWidening`, not a clipped grant.

use hh_compiler::plan::PinnedRef;
use hh_hir::records::{Grant, GrantConstraints};
use hh_hir::refs::Ref;
use hh_provenance::{AuthorityClass, ProvenanceRecord};
use hh_wire::json::Json;

use crate::args::scope_covers;
use crate::handle::{AuthorityHandle, HandleId, HandleValidity, OriginBasis};
use crate::mint::environment_denied;
use crate::table::HandleTable;

/// The `delegate` error sum (§2.1).
#[derive(Debug, Clone, PartialEq)]
pub enum DelegateError {
    /// A requested grant is not ⊆ any `delegable` grant of the parent.
    AuthorityWidening {
        /// The widening grant's domain.
        domain: hh_hir::EffectDomain,
    },
    /// The parent handle is not `delegable` (no delegable grant covered the
    /// request).
    NotDelegable {
        /// The parent handle id.
        handle_id: String,
    },
    /// `budget_req` exceeds what the parent covers.
    BudgetExceedsParent,
    /// `requested_ceiling > parent.ceiling`, or an `environment` mint carried a
    /// denied domain.
    CeilingExceeded,
    /// The parent handle is revoked/expired (delegation needs a live parent).
    ParentNotLive {
        /// The parent handle id.
        handle_id: String,
    },
}

/// `child_spec` — the requested child authority (§2.1 `delegate(parent,
/// child_spec, requested, budget_req, at)`): who the child is, what it asks
/// for, and at what ceiling.
#[derive(Debug, Clone)]
pub struct ChildSpec {
    /// The child's `Ref<AgentProcess>` (the new handle's `holder`).
    pub holder: Ref,
    /// The requested grants.
    pub requested: Vec<Grant>,
    /// The requested ceiling (`min`ed with the parent's).
    pub ceiling: AuthorityClass,
    /// The requested budget bound.
    pub budget_req: Option<Json>,
}

/// `delegate(table, parent_id, spec, coords, alloc)` — produces the child
/// handle set: each requested grant the parent covers yields one
/// `delegation`-basis handle with `parent_handle` set.
/// `ceiling = min(requested, parent.ceiling)`; the `budget_req` must be ⊆ the
/// covering grants' budget constraints.
///
/// `coords` supplies the live `(effect_id, turn_id, run_id, session)`
/// coordinates the parent's `validity` is checked against.
pub fn delegate(
    table: &HandleTable,
    parent_id: &HandleId,
    spec: &ChildSpec,
    coords: &LiveCoords,
    alloc: &mut dyn FnMut(&str) -> String,
) -> Result<Vec<AuthorityHandle>, DelegateError> {
    let requested = &spec.requested;
    let requested_ceiling = spec.ceiling;
    let parent = table
        .get(parent_id)
        .ok_or_else(|| DelegateError::ParentNotLive {
            handle_id: parent_id.as_str().to_string(),
        })?;
    if !parent.is_live(
        &coords.effect_id,
        &coords.turn_id,
        &coords.run_id,
        &coords.session,
    ) {
        return Err(DelegateError::ParentNotLive {
            handle_id: parent_id.as_str().to_string(),
        });
    }
    if requested_ceiling > parent.ceiling {
        return Err(DelegateError::CeilingExceeded);
    }
    // The `environment` floor (§2.5 row 1).
    if requested_ceiling <= AuthorityClass::Environment
        && requested
            .iter()
            .any(|g| environment_denied(g.effect.domain))
    {
        return Err(DelegateError::CeilingExceeded);
    }
    // Budget attenuation: `budget_req` must be ⊆ every covering grant's budget
    // (the covering check below enforces per-grant; a global `budget_req`
    // against a budget-constrained parent is compared structurally).
    for g in requested {
        let covering: Vec<&Grant> = parent
            .grants
            .iter()
            .filter(|pg| grant_covers(pg, g))
            .collect();
        if covering.is_empty() {
            // Distinguish NotDelegable (a covering grant exists but isn't
            // delegable) from AuthorityWidening (none covers at all).
            let covered_non_delegable = parent
                .grants
                .iter()
                .any(|pg| grant_covers_no_delegable(pg, g));
            return Err(if covered_non_delegable {
                DelegateError::NotDelegable {
                    handle_id: parent_id.as_str().to_string(),
                }
            } else {
                DelegateError::AuthorityWidening {
                    domain: g.effect.domain,
                }
            });
        }
        if let Some(req) = spec.budget_req.as_ref() {
            if !covering
                .iter()
                .any(|pg| budget_covers(pg.constraints.budget.as_ref(), req))
            {
                return Err(DelegateError::BudgetExceedsParent);
            }
        }
    }
    let handle_id = alloc("hnd");
    let event_id = alloc("evt");
    Ok(vec![AuthorityHandle {
        handle_id: HandleId(handle_id),
        permission_ref: PinnedRef {
            semantic_id: parent.permission_ref.semantic_id.clone(),
            version_id: parent.permission_ref.version_id.clone(),
        },
        holder: spec.holder.clone(),
        issuer: ProvenanceRecord::kernel("hh-monitor/delegate", coords.seq),
        grants: requested.to_vec(),
        ceiling: requested_ceiling.min(parent.ceiling),
        validity: HandleValidity {
            issued_at: event_id,
            expires_at: parent.validity.expires_at.clone(),
            revoked_by: None,
        },
        parent_handle: Some(parent_id.clone()),
        delegable: requested.iter().all(|g| g.delegable) && parent.delegable,
        origin_basis: OriginBasis::Delegation,
        basis_ref: parent_id.as_str().to_string(),
        budget_ref: None,
    }])
}

/// The live coordinates a liveness check reads (effect/turn/run/session +
/// `seq` for the minted provenance's `created_at`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveCoords {
    /// The current effect scope.
    pub effect_id: String,
    /// The current turn.
    pub turn_id: String,
    /// The run.
    pub run_id: String,
    /// The session ref (empty when none is in force).
    pub session: String,
    /// The current seq (logical time).
    pub seq: u64,
}

/// `grant_covers(parent, child)` — the I-H4 attenuation check including the
/// `delegable` requirement.
pub fn grant_covers(parent: &Grant, child: &Grant) -> bool {
    parent.delegable && grant_covers_no_delegable(parent, child)
}

/// The structural half of `grant_covers` — same domain, class covers, scope ⊆,
/// constraints tightening.
fn grant_covers_no_delegable(parent: &Grant, child: &Grant) -> bool {
    parent.effect.covers(&child.effect)
        && scope_covers(&parent.scope, Some(&child.scope))
        && constraints_tighten(&parent.constraints, &child.constraints)
}

/// `constraints_tighten(parent, child)` — the child may only tighten:
/// `count ≤`, `time ≤`, budget ⊆ (a parent bound must appear, tighter or
/// equal, on the child).
fn constraints_tighten(parent: &GrantConstraints, child: &GrantConstraints) -> bool {
    bound_le(parent.count, child.count)
        && bound_le(parent.time, child.time)
        && budget_sub(&parent.budget, &child.budget)
}

/// `le(parent_bound, child_bound)` — the child must bound at ≤ where the
/// parent bounds.
fn bound_le(parent: Option<u64>, child: Option<u64>) -> bool {
    match (parent, child) {
        (None, _) => true,
        (Some(p), Some(c)) => c <= p,
        (Some(_), None) => false,
    }
}

/// `budget_sub(parent, child)` — a parent `budget` member must be matched by a
/// child `budget` that bounds it no wider: every numeric leaf the parent
/// records must appear in the child at `≤`.
fn budget_sub(parent: &Option<Json>, child: &Option<Json>) -> bool {
    match (parent, child) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(p), Some(c)) => json_le(p, c),
    }
}

/// `budget_covers(parent_budget, request)` — the requested budget must be ⊆
/// the parent's budget bound.
fn budget_covers(parent: Option<&Json>, req: &Json) -> bool {
    match parent {
        None => true, // unconstrained parent covers any request
        Some(p) => json_le(p, req),
    }
}

/// `json_le(bound, request)` — every numeric member of `bound` must appear in
/// `request` at `≤`; non-numeric members must match verbatim. A `request`
/// missing a bounded member widens (false).
fn json_le(bound: &Json, req: &Json) -> bool {
    match (bound, req) {
        (Json::Obj(b), Json::Obj(r)) => b
            .iter()
            .all(|(k, bv)| r.get(k).map(|rv| json_le(bv, rv)).unwrap_or(false)),
        (Json::Int(b), Json::Int(r)) => r <= b,
        (a, b) => a == b,
    }
}
