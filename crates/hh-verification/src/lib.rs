//! `hh-verification` — the C0/Stage-1 verification-plane substrate (spec §5f,
//! R-2.7.2a, R-2.7.1⁰, R-2.7.3⁰; ADR-0109…0117).
//!
//! The plane turns what the model *says* about a run into typed claims, binds
//! each claim to authoritative handles, records a four-valued agreement, and
//! installs the kernel floors that refuse to let a false completion propagate
//! — beside the `Validator` contract (typed verdicts over kernel-built
//! evidence bundles) and the independent-critic declarations.
//!
//! Evidence beats claims: a claim is an `Observation{source = model_claim}` at
//! `authority = delegate`, `evidence_class = claimed` — never evidence
//! ([`evidence::BundleError::ClaimOnlyEvidence`]); verdicts never endorse,
//! confer authority, or rewrite the judged observation (I-V3); a `diverge`
//! cites ≥ 1 handle record, never the claim's own text; no `hold`/`veto` may
//! be caused solely by a `parsed`/`judged` record (F7).
//!
//! ## Modules
//!
//! - [`vocab`] — every closed sum the plane owns (per-dialect closed sets).
//! - [`claims`] — `Claim`, `AuthoritativeHandle`, `ReconciliationRecord`,
//!   `SeverityRecord`, `ReconciliationNotice`, the `bind` kind table and the
//!   pure `ledger_only` reconcile fold.
//! - [`gate`] — `ValidatesRecord`, `AcceptanceCriterion`, `TaskContract`,
//!   `VerificationSummary`, `GateResult`, Γ with the kernel floors F2–F7 as
//!   constants, `validate_gamma`, `evaluate_gate`.
//! - [`evidence`] — `EvidenceHandle`, `EvidenceItem`, `EvidenceBundle`,
//!   `OmissionRecord`, `inputs_digest`, the `collect`/admission checks.
//! - [`validators`] — `ValidatorDeclaration` + `declare`/`bind`, `Verdict`,
//!   `Finding`, the kernel local checks (a)–(c) ((d) is Stage 2's).
//! - [`critics`] — `CriticDeclaration`, `IndependenceVector` + per-use
//!   minimums, `CriticVerdict`, `CalibrationRecord`, `declare_critic`,
//!   `consume`, the `ProgrammaticCritic` contract.
//! - [`events`] — the `verification.*` canonical payload builders.
//! - [`metrics`] — the ADR-0114 D1 deterministic declaration register.
//!
//! ## Staging (ADR-0109…0117 field (e); spec §5f.* §9)
//!
//! Stage 1 lands schemas, event payloads, Γ with floors as kernel constants,
//! the deterministic declaration-level checks and pure folds, the class/
//! dimension/metric registrations and the programmatic-critic contract. The
//! gate wired into `Driver::finish`, the ledger-only D2/D3/D5/D6 detectors
//! over a live run, the `TaskContract` projection at `seal`, veto-predicate
//! evaluation, judged validators/critics and the `probe`/`resume` halves are
//! spec-staged to Stage 2/3/4 — tracked as DF-S1.21-* rows.

pub mod bind;
pub mod claims;
pub mod critics;
pub mod evalfold;
pub mod events;
pub mod evidence;
pub mod gate;
pub mod metrics;
pub mod validators;
pub mod vocab;

/// The `reconciliation.holds` budget dimension spelling (F4; ADR-0113 D4 —
/// a gauge cap on the run's `BudgetNode`, registered in
/// `hh_ontology::dimensions`).
pub const RECONCILIATION_HOLDS: &str = "reconciliation.holds";
