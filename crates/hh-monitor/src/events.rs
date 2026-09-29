//! The `security.permission.*` payload builders and the `granted` decoder the
//! handle-table projection reads (§5g.1 §3; ADR-0051 D9). Every event is
//! audit-grade — kernel producer, mandatory provenance — and carries
//! content-free fields only (CC3; the refusal text is profile-rendered from the
//! closed `DenyReason`, never monitor-authored prose).

use hh_compiler::plan::PinnedRef;
use hh_hir::refs::{Ref, RefVersion};
use hh_ledger::event::EventEnvelope;
use hh_provenance::{AuthorityClass, ProvenanceRecord};
use hh_wire::json::Json;

use crate::decision::KernelDecision;
use crate::handle::{AuthorityHandle, HandleExpiry, HandleId, HandleValidity, OriginBasis};

fn str_at<'a>(j: &'a Json, k: &str) -> Option<&'a str> {
    j.get(k).and_then(Json::as_str)
}

/// The `security.permission.granted` payload for `h` (§5g.1 §3 — every member
/// of the record, plus `scope`/`authority_delta` the audit row carries).
pub fn granted_payload(h: &AuthorityHandle) -> Json {
    Json::obj([
        ("handle_id", Json::str(h.handle_id.as_str())),
        (
            "permission_ref",
            Json::obj([
                (
                    "semantic_id",
                    Json::str(h.permission_ref.semantic_id.clone()),
                ),
                ("version_id", Json::str(h.permission_ref.version_id.clone())),
            ]),
        ),
        ("holder", Json::str(h.holder.semantic_id.clone())),
        ("issuer", h.issuer.to_json()),
        (
            "grants",
            Json::Arr(
                h.grants
                    .iter()
                    .map(|g| hh_hir::grant_json(g, false))
                    .collect(),
            ),
        ),
        ("ceiling", Json::str(h.ceiling.as_str())),
        (
            "validity",
            Json::obj([
                ("issued_at", Json::str(h.validity.issued_at.clone())),
                (
                    "expires_at",
                    h.validity
                        .expires_at
                        .as_ref()
                        .map(HandleExpiry::to_json)
                        .unwrap_or(Json::Null),
                ),
            ]),
        ),
        (
            "parent_handle",
            h.parent_handle
                .as_ref()
                .map(|p| Json::str(p.as_str()))
                .unwrap_or(Json::Null),
        ),
        ("delegable", Json::Bool(h.delegable)),
        ("origin_basis", Json::str(h.origin_basis.as_str())),
        ("scope", Json::str(h.scope.as_str())),
        ("basis_ref", Json::str(h.basis_ref.clone())),
        (
            "budget_ref",
            h.budget_ref
                .as_ref()
                .map(|b| Json::str(b.clone()))
                .unwrap_or(Json::Null),
        ),
        ("authority_delta", Json::str("none")),
    ])
}

/// The `security.permission.revoked` payload `{handle_id, cascade[], reason,
/// revoker}` — `reason` is a closed tag, not prose.
pub fn revoked_payload(id: &HandleId, cascade: &[HandleId], reason: &str, revoker: &str) -> Json {
    Json::obj([
        ("handle_id", Json::str(id.as_str())),
        (
            "cascade",
            Json::Arr(cascade.iter().map(|c| Json::str(c.as_str())).collect()),
        ),
        ("reason", Json::str(reason)),
        ("revoker", Json::str(revoker)),
    ])
}

/// The `security.permission.decided` payload — the full `KernelDecision` plus
/// the attempt coordinate the complete-mediation gate reads.
pub fn decided_payload(d: &KernelDecision, attempt_no: u64, proposal_ref: &str) -> Json {
    let mut m = match d.to_json() {
        Json::Obj(m) => m,
        _ => unreachable!("KernelDecision::to_json is an object"),
    };
    m.insert("attempt_no".to_string(), Json::Int(attempt_no as i64));
    m.insert("proposal".to_string(), Json::str(proposal_ref));
    m.insert(
        "taint".to_string(),
        Json::Arr(d.taint.iter().map(|t| Json::str(t.as_string())).collect()),
    );
    Json::Obj(m)
}

/// Decode a `security.permission.granted` envelope back into an
/// `AuthorityHandle` — the fold's half of the projection (a malformed row is
/// skipped: the durable form was validated at append; `project` never guesses).
pub fn handle_from_granted(env: &EventEnvelope) -> Option<AuthorityHandle> {
    handle_from_granted_payload(&env.payload, &env.event_id)
}

/// Decode a `security.permission.granted` **payload** (no envelope) back into
/// an `AuthorityHandle` — `granted_event_id` supplies the holder's pin
/// (the `RefVersion::Pinned` the envelope's `event_id` carried). The
/// out-of-process `authorize` codec's read direction (AC-R-2.8.1-16).
pub fn handle_from_granted_payload(p: &Json, granted_event_id: &str) -> Option<AuthorityHandle> {
    let handle_id = HandleId::parse(str_at(p, "handle_id")?)?;
    let pref = p.get("permission_ref")?;
    let permission_ref = PinnedRef {
        semantic_id: str_at(pref, "semantic_id")?.to_string(),
        version_id: str_at(pref, "version_id")?.to_string(),
    };
    let holder = Ref {
        semantic_id: str_at(p, "holder")?.to_string(),
        version: RefVersion::Pinned(granted_event_id.to_string()),
    };
    let issuer = ProvenanceRecord::from_json(p.get("issuer")?).ok()?;
    let grants = match p.get("grants")? {
        Json::Arr(rows) => rows
            .iter()
            .enumerate()
            .map(|(i, g)| hh_hir::grant_from_json(g, &format!("grants[{i}]")).ok())
            .collect::<Option<Vec<_>>>()?,
        _ => return None,
    };
    let ceiling = AuthorityClass::parse(str_at(p, "ceiling")?)?;
    let vj = p.get("validity")?;
    let validity = HandleValidity {
        issued_at: str_at(vj, "issued_at")?.to_string(),
        expires_at: vj.get("expires_at").and_then(HandleExpiry::from_json),
        revoked_by: None,
    };
    Some(AuthorityHandle {
        handle_id,
        permission_ref,
        holder,
        issuer,
        grants,
        ceiling,
        validity,
        parent_handle: p
            .get("parent_handle")
            .and_then(Json::as_str)
            .and_then(HandleId::parse),
        delegable: matches!(p.get("delegable"), Some(Json::Bool(true))),
        origin_basis: OriginBasis::parse(str_at(p, "origin_basis")?)?,
        basis_ref: str_at(p, "basis_ref")?.to_string(),
        budget_ref: p.get("budget_ref").and_then(Json::as_str).map(String::from),
        // `scope` joined the dossier at S1.15 — a stored row without it is a
        // seal/delegation-minted handle, in force for the run: `session`.
        scope: str_at(p, "scope")
            .and_then(crate::decision::DecisionScope::parse)
            .unwrap_or(crate::decision::DecisionScope::Session),
    })
}
