//! Hand-stamped provenance over the **canonical** §8.1 model (R-2.1.5; §8.1; §9.1).
//! **Throwaway Stage-0 subset — thin adapter, no second record.**
//!
//! At Stage 0 the baseline does not *derive* authority from a runtime (that is the Stage-1
//! reference monitor, R-2.8.1). It **hand-stamps** the three provenance origins the ladder
//! calls for — `principal`, `definition`, `external` (§9.1 R-2.1.2/R-2.1.5 slice: "Stage 0 may
//! hand-stamp `principal`/`definition`/`external`") — but the stamp is the ONE canonical
//! [`hh_provenance::ProvenanceRecord`] (CC1: exactly one provenance record type in the tree —
//! the local `ProvenanceRecord`/`AuthorityClass` defined here at Stage 0 are retired).
//!
//! The stamps mint through the canonical paths, so every baseline record `validate`s under
//! the real rules:
//!
//! - [`principal`] — `Origin::human(.., principal)` mints `principal` in the table.
//! - [`definition`] — `definition` is minted **only at `seal` or by a `pin` endorsement**, so
//!   the hand-stamp carries a self-consistent `seal` [`Attestation`]: the harness author's
//!   stand-in for the seal the real `seal` operation would record. `minted_ceiling` then
//!   permits `definition` and `ProvenanceRecord::validate` accepts it.
//! - [`external`] — `Origin::tool` mints `external` (DF-S1.3-3: `environment` requires a
//!   closed-schema value from a closed-world tool declared in the *sealed* definition, which
//!   does not exist at Stage 0/1). Lifted content can never confer more (CC2).
//!
//! CC2 holds even here: authority is never read from content and never widened. A hand-stamp
//! is a fixed value set by the harness author, not a claim lifted from a payload; lifted
//! content sits at `external` and can only attenuate (never confer) authority.
//!
//! **Canonical-order note (semantic correction, ADR-0230):** the retired Stage-0 subset
//! ordered `external < definition < principal`; the canonical order is
//! `external < … < principal < definition`. The `context_label` meet in
//! [`crate::context`] now yields `principal` (not `definition`) for a definition+principal
//! context — the corrected semantics: the principal's task text caps the assembled context.

pub use hh_provenance::{
    Attestation, AttestationAnchor, AttestationKind, AuthorityClass, HumanRole, Origin,
    PersistenceScope, ProvenanceRecord,
};

/// The logical `created_at` every baseline hand-stamp carries (seq 0 — stamped at authoring
/// time, before any run event).
const HAND_STAMP_SEQ: u64 = 0;

/// Stamp `principal` authority — the launching human/operator (the task, the budget, the
/// credential grant). `Origin::human(.., Principal)` mints `principal` in the table.
pub fn principal(author_ref: impl Into<String>) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::human(author_ref, HumanRole::Principal),
        PersistenceScope::Run,
        HAND_STAMP_SEQ,
    )
}

/// Stamp `definition` authority — the hand-authored harness definition stands in for a
/// `seal`ed one. `definition` is minted only at `seal`/by `pin` (§8.1 #3), so the stamp is a
/// `human(author)`-origin record carrying a self-consistent `seal` attestation at
/// `authority = definition` — exactly what a real seal records on the record.
pub fn definition(author_ref: impl Into<String>) -> ProvenanceRecord {
    let author_ref = author_ref.into();
    let mut record = ProvenanceRecord::minted(
        Origin::human(author_ref.clone(), HumanRole::Author),
        PersistenceScope::Definition,
        HAND_STAMP_SEQ,
    );
    record.attestation = Some(Attestation {
        kind: AttestationKind::Seal,
        subject_hash: author_ref,
        anchor: AttestationAnchor::Signer("harness:author".into()),
        verified_by: "baseline:author-stamp".into(),
        verified_at: HAND_STAMP_SEQ,
    });
    record.authority = AuthorityClass::Definition;
    record
}

/// Stamp `external` authority — content lifted from a tool result or the model. A `tool`
/// origin mints `external` (DF-S1.3-3). Always the lowest hand-stamped class — lifting can
/// never confer more than `external` (CC2 narrow-or-preserve).
pub fn external(capability: impl Into<String>) -> ProvenanceRecord {
    ProvenanceRecord::minted(
        Origin::tool(capability, "baseline:hand-stamp"),
        PersistenceScope::Run,
        HAND_STAMP_SEQ,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use hh_wire::json::Json;

    #[test]
    fn the_three_stamps_follow_the_canonical_order() {
        // Canonical order: external < principal < definition (ADR-0033 D2).
        assert!(AuthorityClass::External < AuthorityClass::Principal);
        assert!(AuthorityClass::Principal < AuthorityClass::Definition);
    }

    #[test]
    fn stamps_carry_their_class_and_validate() {
        let p = principal("operator:cli");
        assert_eq!(p.authority, AuthorityClass::Principal);
        assert!(p.validate(None).is_ok());
        let d = definition("react/minimal@baseline");
        assert_eq!(d.authority, AuthorityClass::Definition);
        // The seal attestation is what legitimizes `definition` above the minted `principal`.
        assert!(d.validate(None).is_ok());
        assert_eq!(
            d.attestation.as_ref().map(|a| a.kind),
            Some(AttestationKind::Seal)
        );
        let e = external("lifted:tool+model");
        assert_eq!(e.authority, AuthorityClass::External);
        assert!(e.validate(None).is_ok());
    }

    #[test]
    fn json_renders_the_canonical_record() {
        let d = definition("react/minimal@baseline");
        let j = d.to_json();
        assert_eq!(
            j.get("authority").and_then(Json::as_str),
            Some("definition")
        );
        // The origin is the canonical structured form, not a bare label.
        let origin = j.get("origin").unwrap();
        assert_eq!(origin.get("kind").and_then(Json::as_str), Some("human"));
        assert_eq!(
            origin.get("author_ref").and_then(Json::as_str),
            Some("react/minimal@baseline")
        );
    }
}
