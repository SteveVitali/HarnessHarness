//! `hh-provenance` — the canonical §8.1 **provenance & authority model** (ticket S1.3,
//! R-2.1.5). One crate owns the *one* `ProvenanceRecord`, the seven `AuthorityClass` values,
//! `default_authority`, the `Label` lattice, the R-TEXT rule, ledger stamping, propagation
//! (P1/P6/P7 + RP), the `seal`/`endorse`/`declassify` basis, and monitor checks 1/2/5/7 —
//! consumed by `hh-identity` (DF-S1.2-1), `hh-kernel`, and every later stage.
//!
//! # Invariants this crate enforces
//!
//! - **CC1 — one scheme per concern.** [`record::ProvenanceRecord`] is the only provenance
//!   record; `hh-identity` *uses* it rather than defining a second one.
//! - **CC2 — authority is conferred, never read.** [`origin::default_authority`] is total and
//!   pure over `(origin, scope, attestation?)` and never inspects a payload's self-declared
//!   class (it never reads `scope` — location cannot elevate); a label rises only through
//!   `security.label.endorsed`/`declassified`/`applied`.
//! - **CC3 — nothing unaccounted.** [`lower::lower`] returns an explicit `lost` list;
//!   [`lower::lift`] stamps the loss as `import`-taint markers; every monitor refusal is a
//!   typed error.
//! - **Monotonicity (monitor check 2 / P6).** A label may only move toward `unverified`
//!   outside the sealed endorsement events — [`label::check_label_transition`].
//! - **The model never endorses** (check 5): a `delegate`-class endorser is always refused.
//! - **Delegate attenuation (check 7 / P2):** after a delegation, no child provenance may sit
//!   above `delegate` in the parent — [`derive::check_delegate_attenuation`].
//!
//! # Modules
//!
//! - [`authority`] — the seven classes, `PersistenceScope`, `TaintTag`, `ReaderSet`.
//! - [`origin`] — `Origin`, `default_authority` (the minting table), `default_text_authority`
//!   (R-TEXT).
//! - [`label`] — `Label` join/meet/leq, P6 `check_label_transition`, `context_label` +
//!   `effective_authority` (P3), the `HirDiff.authority_delta` gate (P6).
//! - [`record`] — `ProvenanceRecord`, `Derivation`, `Attestation`, `verify_attestation`,
//!   `validate`.
//! - [`derive`] — P1 `derive` (multi-input join), `ingest_child_result` (P2), check 7.
//! - [`endorse`] — `EndorsementBasis` (closed list), `endorse`/`declassify`/`seal`/
//!   `apply_label`, the `security.label.*` payloads, check 5.
//! - [`mandatory`] — the §8.1 #7 mandatory-provenance ledger event table.
//! - [`monitor`] — checks 1 (`operand ⊆ envelope`), 2 (monotonic label), 7 (delegate
//!   attenuation).
//! - [`lower`] — P7 `lower`/`lift` (lossy-explicit carriers) and RP `render_role`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod authority;
pub mod derive;
pub mod endorse;
pub mod label;
pub mod lower;
pub mod mandatory;
pub mod monitor;
pub mod origin;
pub mod record;

pub use authority::{AuthorityClass, OpacityReport, PersistenceScope, ReaderSet, TaintTag};
pub use derive::{check_delegate_attenuation, derive, ingest_child_result, DerivationInput};
pub use endorse::{
    apply_label, check_endorsement, declassify, endorse, seal, ContentKind, EndorsementBasis,
    EndorsementError, LabelApplied, LabelDeclassified, LabelEndorsed,
};
pub use label::{
    check_diff_authority_delta, check_label_transition, classify_label_delta, context_label,
    effective_authority, AuthorityDelta, Delivery, DiffAuthorityError, DiffContext,
    EffectiveAuthority, Label, LabelDelta, LabelTransitionError,
};
pub use lower::{
    lift, lower, render_role, Lifted, LowerTarget, Lowered, RolePlacementError, RoleSlot,
};
pub use mandatory::{
    required_provenance, requires_provenance, ProvenanceEventKind, RequiredProvenance,
};
pub use monitor::{
    check_monotonic_label, check_no_delegate_above, check_operand_in_envelope, MonitorError,
    Operand, PermissionEnvelope,
};
pub use origin::{default_authority, default_text_authority, HumanRole, Origin};
pub use record::{
    require_provenance, verify_attestation, Attestation, AttestationAnchor, AttestationKind,
    Derivation, DerivationKind, ProvenanceError, ProvenanceRecord, TrustedAnchors,
};
