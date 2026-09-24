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
//! - [`origin`] — `Origin`, `default_authority`/`default_authority_in` (the minting table
//!   with the sealed-definition [`origin::MintingContext`]), `default_text_authority` (R-TEXT).
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
//! - [`decode`] — the canonical `from_json` decoders (added for S1.5): the schema source
//!   owns both directions (CC7), so a stored/endorsed record parses back byte-identically.
//! - [`flow`] — the C2 slice (§5g.2, R-2.8.2): `FlowContract` + its codec, `L⁺(p)`,
//!   `admit`, `EnforcementClass`, D-ROBUST, the closed `Remedy` set, check 3, and the
//!   closed bounded flow-policy language (`FlowCond`/`FlowRule`).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod authority;
pub mod decode;
pub mod derive;
pub mod endorse;
pub mod flow;
pub mod flow_policy;
pub mod label;
pub mod lower;
pub mod mandatory;
pub mod monitor;
pub mod origin;
pub mod record;

pub use authority::{AuthorityClass, OpacityReport, PersistenceScope, ReaderSet, TaintTag};
pub use decode::DecodeError;
pub use derive::{check_delegate_attenuation, derive, ingest_child_result, DerivationInput};
pub use endorse::{
    apply_label, check_endorsement, declassify, endorse, sanitize_endorse, seal, shape_endorse,
    ContentKind, EndorsementBasis, EndorsementError, LabelApplied, LabelDeclassified,
    LabelEndorsed,
};
pub use flow::{
    admit, apply_contribution, apply_sanitizer_bounds, capacity_bits, check3_relevant,
    check_basis_effect, check_flow, check_reader_coverage, check_sanitizer_effect,
    check_shape_endorsement, d_robust, enumerate_remedies, eval_cond, flow_rule_from_json,
    flow_rule_json, label_bytes, label_from_json, label_json_full, prospective_label,
    resolve_recipients, Admission, AdmissionKind, BasisEffectError, CommittedEffect, Contribution,
    EnforcementClass, EvalError, FlowCond, FlowContract, FlowDecision, FlowError, FlowInput,
    FlowRule, FlowSelector, FlowVerdict, ReadersFrom, ReadersTo, Remedy, RobustnessInput,
    SanitizerBounds, Subject, TaintTagPattern, CAP_MAX_DEFAULT, COND_MAX_DEPTH, COND_MAX_NODES,
    LABEL_MAX_BYTES,
};
pub use flow_policy::{
    apply_policy_edit, check_flow_policy, classify_policy_edit, policy_allows, rules_allow,
    validate_flow_policy, FlowPolicy, FlowPolicyError, PolicyEditClass, PolicyEditError,
    PolicyPoint, ProposalSpace, DENY_REASON_SPELLINGS, FLOW_POLICY_MAX_RULES,
};
pub use label::{
    check_diff_authority_delta, check_label_transition, classify_label_delta, context_label,
    effective_authority, AuthorityDelta, Delivery, DiffAuthorityError, DiffContext,
    EffectiveAuthority, Label, LabelDelta, LabelTransitionError,
};
pub use lower::{
    lift, lift_provenance_meta, lower, lower_provenance_meta, render_role, role_map_collapse,
    Lifted, LowerTarget, Lowered, MetaLift, MetaLiftSource, RoleCollapse, RolePlacementError,
    RoleSlot, META_PROVENANCE_KEY,
};
pub use mandatory::{
    required_provenance, requires_provenance, ProvenanceEventKind, RequiredProvenance,
};
pub use monitor::{
    check_monotonic_label, check_no_delegate_above, check_operand_in_envelope, MonitorError,
    Operand, PermissionEnvelope,
};
pub use origin::{
    default_authority, default_authority_in, default_text_authority, HumanRole, MintingContext,
    Origin,
};
pub use record::{
    require_provenance, verify_attestation, Attestation, AttestationAnchor, AttestationKind,
    Derivation, DerivationKind, ProvenanceError, ProvenanceRecord, TrustedAnchors,
};
