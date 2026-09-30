//! `BundleError` — the closed error sum for the bundle plane (§5h.3 §2
//! error columns; ADR-0139/0140/0141). Validation *diagnostics* live on
//! `BundleValidationReport` (a report, not an error); these are the
//! operation-level refusals.

use std::fmt;

/// The closed refusal sum for `bundle`/`encode`/`decode`/`reproduce`/`import`.
#[derive(Debug, Clone, PartialEq)]
pub enum BundleError {
    /// The subject run is not in the store.
    RunNotFound { run_id: String },
    /// `bundle` on an open (non-durable) run caps at R0 — a refusal when the
    /// caller asked for more.
    RunNotDurable { run_id: String },
    /// Secret material or a credentialed locator in a member (S8; R-ID-7).
    SecretMaterialPresent { detail: String },
    /// `claimed_level` above the basis-derived maximum.
    ReproClaimUnsupported {
        claimed: String,
        max_supported: String,
    },
    /// A manifest reference resolves to nothing (no member, fetch or
    /// unpinned entry).
    ManifestReferenceUnresolved { reference: String },
    /// `decode`/`validate` input is not a `hh-bundle/1` manifest.
    UnknownBundleSchema { detail: String },
    /// A member the manifest names is neither present nor fetchable.
    MemberUnavailable { address: String },
    /// A present member's bytes do not hash back to its address.
    MemberMismatch { address: String },
    /// `reproduce` level above `max_supported_level` (or a class-n/a level).
    LevelUnsupported {
        level: String,
        max_supported: String,
    },
    /// `reproduce` with unmatched `eval_budget` (T-LCD-14).
    UnmatchedBudget { detail: String },
    /// The environment the bundle pins cannot be provisioned here.
    EnvironmentUnavailable { detail: String },
    /// The model snapshot the bundle pins is not available.
    ModelUnavailable { detail: String },
    /// The artefact handed to `import` is not a `ledger_native` bundle.
    FormatUnknown { detail: String },
    /// An IO failure at the directory/archive seam.
    Io { detail: String },
    /// A malformed record decode.
    Malformed { detail: String },
}

impl fmt::Display for BundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BundleError::RunNotFound { run_id } => write!(f, "run_not_found: {run_id}"),
            BundleError::RunNotDurable { run_id } => write!(f, "run_not_durable: {run_id}"),
            BundleError::SecretMaterialPresent { detail } => {
                write!(f, "secret_material_present: {detail}")
            }
            BundleError::ReproClaimUnsupported {
                claimed,
                max_supported,
            } => write!(
                f,
                "repro_claim_unsupported: claimed {claimed} > max_supported {max_supported}"
            ),
            BundleError::ManifestReferenceUnresolved { reference } => {
                write!(f, "manifest_reference_unresolved: {reference}")
            }
            BundleError::UnknownBundleSchema { detail } => {
                write!(f, "unknown_bundle_schema: {detail}")
            }
            BundleError::MemberUnavailable { address } => {
                write!(f, "member_unavailable: {address}")
            }
            BundleError::MemberMismatch { address } => {
                write!(f, "member_mismatch: {address}")
            }
            BundleError::LevelUnsupported {
                level,
                max_supported,
            } => write!(
                f,
                "level_unsupported: {level} (max_supported {max_supported})"
            ),
            BundleError::UnmatchedBudget { detail } => write!(f, "unmatched_budget: {detail}"),
            BundleError::EnvironmentUnavailable { detail } => {
                write!(f, "environment_unavailable: {detail}")
            }
            BundleError::ModelUnavailable { detail } => {
                write!(f, "model_unavailable: {detail}")
            }
            BundleError::FormatUnknown { detail } => write!(f, "format_unknown: {detail}"),
            BundleError::Io { detail } => write!(f, "io: {detail}"),
            BundleError::Malformed { detail } => write!(f, "malformed: {detail}"),
        }
    }
}

impl std::error::Error for BundleError {}
