//! Root minting (§5g.1 §2.1; ADR-0051 H-2/H-3).
//!
//! `mint_root_handles` is the *only* origin of `seal`-basis handles: it walks
//! the sealed definition's `Permission` entities, applies every `authority_cap`
//! constraint the resolved `assembly.constraints` carries (a cap may only lower
//! the ceiling — ADR-0024's MUST-data boundary), and returns the
//! `AuthorityHandle` set the caller appends as `security.permission.granted`
//! rows. It is minted **before** any tool output, repository content or memory
//! is read (H-3) — by construction: the inputs are the `SealedDefinition` (a
//! trusted record), the principal's provenance and an id allocator; no content
//! input exists on the path.
//!
//! `Permission.issuer` is `{authority, reference}` — a declared class plus an
//! identity coordinate, *not* a provenance record. I-H2's `issuer.origin`
//! therefore reads the **node's** provenance origin (the record's conferring
//! origin — at seal it is `definition`-endorsed); `model`/`evolution`/
//! `participant` provenance on the `Permission` node may never confer.

use hh_compiler::plan::PinnedRef;
use hh_hir::kinds::EffectDomain;
use hh_hir::records::{Issuer, KindRecord, PermissionRecord};
use hh_hir::refs::Ref;
use hh_hir::{HirDocument, SealedDefinition};
use hh_provenance::{AuthorityClass, Origin, ProvenanceRecord};
use hh_wire::json::Json;

use crate::handle::{AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis};

/// The mint error sum (§2.1 `mint_root_handles` errors).
#[derive(Debug, Clone, PartialEq)]
pub enum MintError {
    /// `issuer.authority < principal` or the `Permission` node's provenance
    /// origin is `model`/`evolution`/`participant` (I-H2).
    IllegitimateIssuer {
        /// The permission's semantic id.
        permission_id: String,
    },
    /// A `session`-lifetime approval handle without a declared
    /// `ActionPattern` (I-H6's Stage-2 admission leg — the member is admitted
    /// only through a declared pattern).
    SessionScopeRequiresPattern,
}

/// One resolved `authority_cap` — the `subject{of?, ceiling}` record of an
/// `assembly.constraints` member of kind `authority_cap` (ADR-0240 shape).
/// `of` names the entity the cap bounds (a `Permission` semantic id); `of =
/// "*"` or absent is the global cap. Caps only ever lower a minted ceiling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityCap {
    /// The capped entity (`None`/`"*"` = global).
    pub of: Option<String>,
    /// The ceiling.
    pub ceiling: AuthorityClass,
}

/// Read the `authority_cap` rows out of a sealed document's resolved
/// `assembly.constraints` (the assembly section survives `seal` when resolved —
/// §3.1.6). Malformed rows are skipped — `validate_assembly` already enforced
/// the `subject.ceiling` spell at compose; mint never guesses.
fn cap_rows(doc: &HirDocument) -> Vec<AuthorityCap> {
    let mut out = Vec::new();
    let Some(assembly) = &doc.assembly else {
        return out;
    };
    let constraints = match assembly.get("constraints") {
        Some(Json::Arr(cs)) => cs,
        _ => return out,
    };
    for c in constraints {
        if c.get("kind").and_then(Json::as_str) != Some("authority_cap") {
            continue;
        }
        let subject = c.get("subject").cloned().unwrap_or(Json::Null);
        let Some(ceiling) = subject
            .get("ceiling")
            .and_then(Json::as_str)
            .and_then(AuthorityClass::parse)
        else {
            continue;
        };
        let of = subject
            .get("of")
            .and_then(Json::as_str)
            .map(|s| s.to_string());
        out.push(AuthorityCap { of, ceiling });
    }
    out
}

/// `cap_ceiling(perm_id, issuer.authority, caps)` — `min` over the issuer's
/// declared authority and every applicable cap (entity-named and global).
pub fn cap_ceiling(perm_id: &str, issuer: &Issuer, caps: &[AuthorityCap]) -> AuthorityClass {
    let mut ceiling = issuer.authority;
    for c in caps {
        let applies = match &c.of {
            None => true,
            Some(of) => of == "*" || of == perm_id,
        };
        if applies && c.ceiling < ceiling {
            ceiling = c.ceiling;
        }
    }
    ceiling
}

/// The I-H2 conferral check: `issuer.authority ≥ principal.authority` and the
/// `Permission` node's provenance origin ∉ `{model, evolution, participant}`.
fn legitimate_issuer(
    perm: &PermissionRecord,
    node_prov: &ProvenanceRecord,
    principal: &ProvenanceRecord,
) -> bool {
    perm.issuer.authority >= principal.authority
        && !matches!(
            node_prov.origin,
            Origin::Model { .. } | Origin::Evolution { .. } | Origin::Participant { .. }
        )
}

/// `mint_root_handles(sealed, principal, run_id, alloc)` — the §2.1 root set.
/// One handle per `Permission` entity: `origin_basis = seal`, `issuer` = the
/// node's (definition-endorsed) provenance, `basis_ref` = the sealed
/// definition's coordinate, `ceiling` = `min(issuer.authority, caps)`,
/// `expires_at = run` (the default `run` lifetime — I-H6: survives `restore`,
/// dies with the run). `alloc("hnd")`/`alloc("evt")` allocate the handle id and
/// the id of the `granted` event that will carry the row (`issued_at`).
///
/// Returns `(handles, granted event ids)` — the caller wraps each handle in a
/// `security.permission.granted` event ([`crate::events::granted_payload`])
/// carrying `event_id = event_ids[i]`.
pub fn mint_root_handles(
    sealed: &SealedDefinition,
    principal: &ProvenanceRecord,
    run_id: &str,
    alloc: &mut dyn FnMut(&str) -> String,
) -> Result<(Vec<AuthorityHandle>, Vec<String>), MintError> {
    let caps = cap_rows(&sealed.document);
    let mut handles = Vec::new();
    let mut event_ids = Vec::new();
    for node in &sealed.document.nodes {
        let KindRecord::Permission(perm) = &node.semantic else {
            continue;
        };
        let perm_id = node.semantic_id();
        if !legitimate_issuer(perm, &node.provenance, principal) {
            return Err(MintError::IllegitimateIssuer {
                permission_id: perm_id,
            });
        }
        let ceiling = cap_ceiling(&perm_id, &perm.issuer, &caps);
        let handle_id = alloc("hnd");
        let event_id = alloc("evt");
        handles.push(AuthorityHandle {
            handle_id: HandleId(handle_id),
            permission_ref: PinnedRef {
                semantic_id: perm_id,
                version_id: node.version_id(),
            },
            holder: perm.holder.clone(),
            issuer: node.provenance.clone(),
            grants: perm.grants.clone(),
            ceiling,
            validity: HandleValidity {
                issued_at: event_id.clone(),
                expires_at: Some(HandleExpiry::Run(run_id.to_string())),
                revoked_by: None,
            },
            parent_handle: None,
            delegable: !perm.grants.is_empty() && perm.grants.iter().all(|g| g.delegable),
            origin_basis: OriginBasis::Seal,
            basis_ref: format!(
                "{}#{}",
                sealed.definition_ref.semantic_id, sealed.definition_ref.version_id
            ),
            budget_ref: None,
            // Seal-minted roots are in force for the run (`HandleExpiry::Run`)
            // — `session` scope, never `persisted` (a persisted widening needs
            // the human-origin `lifecycle.definition.changed`, ADR-0066 D5).
            scope: crate::decision::DecisionScope::Session,
        });
        event_ids.push(event_id);
    }
    Ok((handles, event_ids))
}

/// `ApprovalMint` — the `respond`-path mint input (§5g.1 §3 + R-2.8.7 §5):
/// a legitimate `allow`/`allow_lease` decision confers an `approval`-basis
/// `AuthorityHandle` on the proposer over the pending's capability. The basis
/// is the *decision record* (`basis_ref = permission_id`), never the response
/// text — I-H1.
#[derive(Debug, Clone)]
pub struct ApprovalMint {
    /// The resolved pending (the basis coordinate).
    pub permission_id: String,
    /// The proposer the handle issues to (`Ref<AgentProcess>`).
    pub holder: Ref,
    /// The endorser's provenance — `human{authority: principal}` or a live
    /// `ApproverGrant` (legitimacy ran at `respond`; mint records it).
    pub issuer: ProvenanceRecord,
    /// The granted effects (the pending capability's grant shape).
    pub grants: Vec<hh_hir::records::Grant>,
    /// The ceiling — the grant's recorded ceiling, never above the pending's
    /// decided risk ceiling (raise-only).
    pub ceiling: AuthorityClass,
    /// The response's decision scope (`once | session | persisted`).
    pub scope: crate::decision::DecisionScope,
    /// The `allow_lease` lease scope (`None` for `allow_once`).
    pub lease_scope: Option<crate::approval::LeaseScope>,
    /// The `ActionPattern` a pattern lease declared (I-H6 — a `session`
    /// lifetime is admitted only through a declared pattern).
    pub lease_pattern: Option<crate::approval::ActionPattern>,
    /// The live coordinates the expiry members name.
    pub effect_id: String,
    /// The live turn.
    pub turn_id: String,
    /// The live run.
    pub run_id: String,
    /// The live session ref (empty when none).
    pub session_ref: String,
    /// The budget node the handle's effects charge, when declared.
    pub budget_ref: Option<String>,
}

/// `mint_approval_handle(m, alloc) → (handle, granted_event_id)` — the C1
/// `approval`-basis mint. `expires_at` follows the response scope: `once` →
/// `effect`, a lease's `turn`/`run`/`session` → the same member;
/// `persisted` bounds to `session` in force — a persisted widening requires
/// the human-origin `lifecycle.definition.changed` (ADR-0066 D5), which this
/// mint never fabricates. A `session` expiry without a declared
/// `ActionPattern` refuses (`SessionScopeRequiresPattern` — I-H6's Stage-2
/// admission leg).
pub fn mint_approval_handle(
    m: &ApprovalMint,
    alloc: &mut dyn FnMut(&str) -> String,
) -> Result<(AuthorityHandle, String), MintError> {
    use crate::approval::LeaseScope;
    let expires = match m.scope {
        crate::decision::DecisionScope::Once => HandleExpiry::Effect(m.effect_id.clone()),
        crate::decision::DecisionScope::Session => match m.lease_scope {
            Some(LeaseScope::Turn) => HandleExpiry::Turn(m.turn_id.clone()),
            Some(LeaseScope::Session) => {
                if m.lease_pattern.is_none() {
                    return Err(MintError::SessionScopeRequiresPattern);
                }
                HandleExpiry::Session(m.session_ref.clone())
            }
            // `allow_lease{scope = run}` and a `session` decision without a
            // lease both bound to the run.
            _ => HandleExpiry::Run(m.run_id.clone()),
        },
        crate::decision::DecisionScope::Persisted => {
            if m.session_ref.is_empty() {
                return Err(MintError::SessionScopeRequiresPattern);
            }
            HandleExpiry::Session(m.session_ref.clone())
        }
    };
    let handle_id = alloc("hnd");
    let event_id = alloc("evt");
    Ok((
        AuthorityHandle {
            handle_id: HandleId(handle_id),
            permission_ref: PinnedRef {
                semantic_id: m.permission_id.clone(),
                version_id: m.permission_id.clone(),
            },
            holder: m.holder.clone(),
            issuer: m.issuer.clone(),
            grants: m.grants.clone(),
            ceiling: m.ceiling,
            validity: HandleValidity {
                issued_at: event_id.clone(),
                expires_at: Some(expires),
                revoked_by: None,
            },
            parent_handle: None,
            delegable: false,
            origin_basis: OriginBasis::Approval,
            basis_ref: m.permission_id.clone(),
            budget_ref: m.budget_ref.clone(),
            scope: m.scope,
        },
        event_id,
    ))
}

/// `mint_preauthorization_handles(sealed, holder, run_id, alloc)` — the
/// `pre_authorize` half of ADR-0053 D5: every `HarnessRule` whose action is
/// `pre_authorize{grants, scope?}` mints a `policy_rule`-basis handle over the
/// declared grants at seal. The handle confers `pre_authorized` at `authorize`
/// (the unattended-`ask` exception) and serves the reviewer chain's
/// `policy_rule` stage. Reviewers may narrow or withdraw — never widen
/// (raise-only admission).
///
/// The `grants` member decodes through the canonical grant codec; a malformed
/// row skips the rule (the sealed definition validated it — mint never
/// guesses). `expires_at = run` — a pre-authorization never outlives the run.
pub fn mint_preauthorization_handles(
    sealed: &SealedDefinition,
    holder: &Ref,
    issuer: &ProvenanceRecord,
    run_id: &str,
    alloc: &mut dyn FnMut(&str) -> String,
) -> Vec<(AuthorityHandle, String)> {
    let mut out = Vec::new();
    for node in &sealed.document.nodes {
        let KindRecord::HarnessRule(rule) = &node.semantic else {
            continue;
        };
        let hh_hir::records::RuleAction::PreAuthorize(spec) = &rule.action else {
            continue;
        };
        let grants: Vec<hh_hir::records::Grant> = match spec.get("grants") {
            Some(Json::Arr(rows)) => rows
                .iter()
                .enumerate()
                .map(|(i, g)| hh_hir::grant_from_json(g, &format!("grants[{i}]")).ok())
                .collect::<Option<Vec<_>>>()
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        if grants.is_empty() {
            continue;
        }
        let handle_id = alloc("hnd");
        let event_id = alloc("evt");
        out.push((
            AuthorityHandle {
                handle_id: HandleId(handle_id),
                permission_ref: PinnedRef {
                    semantic_id: rule.rule_id.clone(),
                    version_id: node.version_id(),
                },
                holder: holder.clone(),
                issuer: issuer.clone(),
                grants,
                ceiling: AuthorityClass::Delegate,
                validity: HandleValidity {
                    issued_at: event_id.clone(),
                    expires_at: Some(HandleExpiry::Run(run_id.to_string())),
                    revoked_by: None,
                },
                parent_handle: None,
                delegable: false,
                origin_basis: OriginBasis::PolicyRule,
                basis_ref: rule.rule_id.clone(),
                budget_ref: None,
                scope: crate::decision::DecisionScope::Session,
            },
            event_id,
        ));
    }
    out
}

/// Whether `domain` is in the mandatory `environment`-mint exclusion set
/// (§2.5 row 1: `environment`-minted handles never carry these).
pub fn environment_denied(domain: EffectDomain) -> bool {
    matches!(
        domain,
        EffectDomain::FsWrite | EffectDomain::Exec | EffectDomain::SecretAccess
    )
}

/// `Ref<AgentProcess>` identity — the holder comparison key (`semantic_id`;
/// the version coordinate is the handle's pin, the identity is the semantic
/// coordinate).
pub fn holder_id(r: &Ref) -> &str {
    &r.semantic_id
}
