//! `AuthorityHandle` — the kernel-owned capability record (§5g.1 §3; ADR-0051
//! D1/D4). A handle is **unforgeable by construction**: it exists only as a row
//! in the kernel's [`crate::table::HandleTable`], minted by `granted` events the
//! kernel appends. A `HandleId` is an allocated, opaque, time-ordered identity
//! coordinate — a string that *spells* one in a `Text` leaf, a tool result or a
//! forged payload confers nothing (I-H1; AC-R-2.8.1-2).

use hh_compiler::plan::PinnedRef;
use hh_hir::records::Grant;
use hh_hir::refs::Ref;
use hh_provenance::{AuthorityClass, ProvenanceRecord};
use hh_wire::json::Json;

/// `HandleId` — allocated, opaque, time-ordered (ADR-0051 D1). The `hnd-`
/// spelling is a formatting convention only: `HandleId::parse` accepts any
/// `hnd-*` string, but a parsed id is **not** authority — only a live row in
/// the `HandleTable` is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HandleId(pub String);

impl HandleId {
    /// The id spelling. `parse` never confers: it only recognises the shape for
    /// `HandleLeak` scans and event decoding.
    pub fn parse(s: &str) -> Option<HandleId> {
        if s.starts_with("hnd-") && s.len() > 4 {
            Some(HandleId(s.to_string()))
        } else {
            None
        }
    }

    /// The raw spelling.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// `origin_basis ∈ {seal, approval, policy_rule, delegation, pin}` — the closed
/// sum recording *how* the handle was conferred (§5g.1 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginBasis {
    /// Minted at `seal` from the definition's `Permission` entities.
    Seal,
    /// Minted by an approval (R-2.8.7 — Stage 2).
    Approval,
    /// Minted by a `definition`-issued pre-authorization rule (Stage 2).
    PolicyRule,
    /// Minted by `delegate` (attenuated from a parent handle).
    Delegation,
    /// Minted by a `pin` endorsement (R-2.8.5).
    Pin,
}

impl OriginBasis {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            OriginBasis::Seal => "seal",
            OriginBasis::Approval => "approval",
            OriginBasis::PolicyRule => "policy_rule",
            OriginBasis::Delegation => "delegation",
            OriginBasis::Pin => "pin",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<OriginBasis> {
        match s {
            "seal" => Some(OriginBasis::Seal),
            "approval" => Some(OriginBasis::Approval),
            "policy_rule" => Some(OriginBasis::PolicyRule),
            "delegation" => Some(OriginBasis::Delegation),
            "pin" => Some(OriginBasis::Pin),
            _ => None,
        }
    }
}

/// `expires_at` — the closed lifetime sum `{effect_id | turn_id | run_id |
/// session_ref}` (I-H6). The `Session` member is admitted only through an
/// R-2.8.7-declared `ActionPattern` (Stage 2); Stage-1 minting never emits it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandleExpiry {
    /// Single-effect lifetime (the `approval`-basis default).
    Effect(String),
    /// Turn lifetime.
    Turn(String),
    /// Run lifetime — survives `restore`, not `continue_goal`/`fork` (I-H6).
    Run(String),
    /// Session lifetime — R-2.8.7 `ActionPattern`-declared only.
    Session(String),
}

impl HandleExpiry {
    /// The canonical `{kind, ref}` JSON.
    pub fn to_json(&self) -> Json {
        let (kind, r) = match self {
            HandleExpiry::Effect(r) => ("effect_id", r),
            HandleExpiry::Turn(r) => ("turn_id", r),
            HandleExpiry::Run(r) => ("run_id", r),
            HandleExpiry::Session(r) => ("session_ref", r),
        };
        Json::obj([(kind, Json::str(r.clone()))])
    }

    /// Parse the canonical form — `None` on any other shape (never coerced).
    pub fn from_json(j: &Json) -> Option<HandleExpiry> {
        for (k, mk) in [
            (
                "effect_id",
                HandleExpiry::Effect as fn(String) -> HandleExpiry,
            ),
            ("turn_id", HandleExpiry::Turn),
            ("run_id", HandleExpiry::Run),
            ("session_ref", HandleExpiry::Session),
        ] {
            if let Some(s) = j.get(k).and_then(Json::as_str) {
                return Some(mk(s.to_string()));
            }
        }
        None
    }

    /// Whether the handle is expired at `at` given the *current* coordinates —
    /// expiry members name the boundary (effect/turn/run/session) the handle
    /// dies with; the caller supplies the live coordinates.
    pub fn expired(&self, effect_id: &str, turn_id: &str, run_id: &str, session: &str) -> bool {
        match self {
            HandleExpiry::Effect(r) => r != effect_id,
            HandleExpiry::Turn(r) => r != turn_id,
            HandleExpiry::Run(r) => r != run_id,
            HandleExpiry::Session(r) => r != session,
        }
    }
}

/// `validity{issued_at, expires_at?, revoked_by?}` (§5g.1 §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandleValidity {
    /// The minting event.
    pub issued_at: String,
    /// The lifetime bound, when declared.
    pub expires_at: Option<HandleExpiry>,
    /// The revoking event, once revoked.
    pub revoked_by: Option<String>,
}

/// `AuthorityHandle` — the §5g.1 §3 record verbatim:
/// `{handle_id, permission_ref, holder, issuer, grants, ceiling, validity,
/// parent_handle?, delegable, origin_basis, basis_ref, budget_ref?}`.
/// `Grant` is ADR-0016's `Permission.grants[]` element unchanged (CC1 — no
/// second grant vocabulary).
#[derive(Debug, Clone, PartialEq)]
pub struct AuthorityHandle {
    /// The allocated opaque id.
    pub handle_id: HandleId,
    /// The `Permission` this handle was minted from.
    pub permission_ref: PinnedRef,
    /// The `Ref<AgentProcess>` the handle is issued to.
    pub holder: Ref,
    /// The conferring provenance — `issuer.authority ≥ principal` and
    /// `issuer.origin ∉ {model, evolution, participant}` (I-H2).
    pub issuer: ProvenanceRecord,
    /// The granted effects.
    pub grants: Vec<Grant>,
    /// The authority ceiling — `min(issuer.authority, every applicable
    /// authority_cap ceiling)` at mint.
    pub ceiling: AuthorityClass,
    /// Validity window.
    pub validity: HandleValidity,
    /// The parent handle for `delegation`-basis handles.
    pub parent_handle: Option<HandleId>,
    /// Whether this handle may be delegated onward.
    pub delegable: bool,
    /// How the handle was conferred.
    pub origin_basis: OriginBasis,
    /// The basis coordinate (seal: the `definition_ref`; delegation: the
    /// `control.subagent.spawned` event; approval: the `permission_id`).
    pub basis_ref: String,
    /// The budget node the handle's effects charge, when declared.
    pub budget_ref: Option<String>,
}

impl AuthorityHandle {
    /// Whether the handle is live (not revoked) at the given coordinates.
    pub fn is_live(&self, effect_id: &str, turn_id: &str, run_id: &str, session: &str) -> bool {
        self.validity.revoked_by.is_none()
            && !self
                .validity
                .expires_at
                .as_ref()
                .map(|e| e.expired(effect_id, turn_id, run_id, session))
                .unwrap_or(false)
    }
}
