//! `hh-bundle` — the reproducible-bundle plane (spec §5h.3; R-2.9.3⁰;
//! ticket S3.1; ADR-0139/0140/0141).
//!
//! Layering (ADR-0038's three-layer split):
//! - **layer M** — [`manifest`]: the `hh-bundle/1` `BundleManifest`
//!   (header + ADR-0038's section list + the reproducibility block) and
//!   `MemberRef`/`Claim`/`Unpinned`/`FetchEntry`/`BundlePolicy`. Identity
//!   is the tree rule: `version_id` = `idp/1` over
//!   `canonical(manifest − version_id)` — archives and directory layouts
//!   are transport, never identity.
//! - **layer P** — [`export`]: the chunked `LedgerExport` (canonical
//!   event pages by seq range, a page-address tree, checkpoints, blob
//!   index, head, lineage prefixes) built over `Store` reads.
//! - **layer A** — [`codec`]: the archive bytes (directory layout and
//!   the deterministic `HHB1` container), transport only.
//!
//! [`assemble::assemble`] is `bundle(kind = run)`; [`validate`] is the
//! staged `validate_bundle`/`check_completeness` (S1–S8, no fail-fast;
//! S9 is publication-time, Stage 4). [`levels::derive`] is the shared
//! closed `B-R*` requirement evaluator used by both assembly and
//! validation. [`import::lift`] stamps every lifted fact
//! `authority = unverified` under `origin = import(...)` and emits the
//! mandatory `MappingReport` + `ImportRecord` (ADR-0141). Source bundles
//! are never rewritten (supersession by new `version_id` only).

pub mod assemble;
pub mod codec;
pub mod error;
pub mod export;
pub mod import;
pub mod levels;
pub mod manifest;
pub mod repro;
pub mod runrefs;
pub mod validate;

pub use assemble::{assemble, AssembleInputs, Assembled, CompileOutcome};
pub use codec::{decode, decode_container, decode_dir, encode_container, encode_dir, Decoded};
pub use error::BundleError;
pub use export::build_ledger_export;
pub use import::{lift, ImportEvent, ImportLift, NATIVE_FORMAT};
pub use manifest::{
    BasisSatisfaction, BlobIndexEntry, BundleManifest, BundlePolicy, Claim, FetchEntry,
    LedgerExport, LevelBasis, MemberRef, MemberStatus, ReproLevel, SubjectSection, Unpinned,
    BUNDLE_IDP, BUNDLE_SCHEMA,
};
pub use repro::{
    budget_limits_equal, fingerprint_drift, snapshot_fingerprint, ReproOutcome, ReproReport,
    REPRO_SCHEMA,
};
pub use runrefs::{declared_run_refs, manifest_refs, run_refs_index};
pub use validate::{
    check_completeness, validate, BundleValidationReport, CheckRow, CheckStatus, StageReport,
    REPORT_SCHEMA,
};
