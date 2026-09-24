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
