//! `hh-identity` — the C0/Stage-1 **identity, versioning & artifact-identity** model (spec §8.3,
//! scope item **R-2.12.1**; ADR-0036/0037/0038).
//!
//! This crate makes "version everything" one contract, and it is the place **CC1 (one scheme per
//! concern) anchors**: every id in the persistent tree — content addresses, version ids, semantic
//! ids, configuration ids and the `hh-embed/1` `schema_hash` — is produced by the *one* `idp/1`
//! construction (`H(idp ∥ domain_tag ∥ canonical)`, [`idp`]) over the *one* canonicalizer
//! (`hh-wire::json` sorted-key compact) and the *one* hash primitive (`hh-wire::sha256`). There
//! is no second canonicalization or hashing scheme after S1.2.
//!
//! What lands here (§8.3 #9, C0/Stage 1): the identity profile [`idp::IDP_1`]; `address` (blob +
//! the one tree rule — [`idp`], [`tree`]), `identify`/`verify` sharing the canonicalizer; the
//! five identity [`kinds`] and N1–N8; [`refs::VersionedRef`]; both configuration ids ([`config`]);
//! the local single-namespace [`names`] index with `publish`/`resolve`; `supersede`/`revoke`
//! records and edges with the derived `StaleIndex` ([`supersede`]); the [`sameness`] ladder
//! L0–L4; the reproducibility levels R0–R3 and [`repro::InstrumentRecord`]; the results/audit
//! [`rowkeys`]; and the golden [`corpus`] (AC-1…-5, -12). Full revocation *enforcement* in
//! `resolve`/retrieval (Stage 2), `BundleManifest`/`reproduce` (Stage 3) and rotation (Stage 4)
//! ride the schemas landed here.
//!
//! CC4: the spec/contracts name no language; this crate is the E1 *build binding* only.

pub mod config;
pub mod corpus;
pub mod idp;
pub mod kinds;
pub mod names;
pub mod record;
pub mod refs;
pub mod repro;
pub mod rowkeys;
pub mod sameness;
pub mod supersede;
pub mod tree;

pub use config::{configuration_id, configuration_version_id};
pub use idp::{
    address, identify_bytes, identify_text, idp_digest, idp_id, parse_id, verify_blob,
    verify_record, ContentAddress, IdError, IdentityProfile, ParsedId, VerifyOutcome, IDP_1,
};
pub use kinds::{AllocatedIdKind, RecordKind};
pub use names::{
    NameHistoryEntry, NameIndex, NameStatus, Namespace, PublishError, ResolveMode, ResolveOutcome,
};
pub use record::{identify, IdentifyError, Identity, Record, Reference};
pub use refs::{Name, NameSelector, Origin, Provenance, VersionedRef};
pub use repro::{
    check_claim, max_supported_level, InstrumentRecord, ReproClaimUnsupported, ReproLevel,
};
pub use rowkeys::{IrRef, ResultsRowKey};
pub use sameness::{
    is_resumable, pools_by_configuration_id, sameness, Delta, DiffClassification, Sameness,
    SamenessLevel,
};
pub use supersede::{
    Lineage, RevocationRecord, StaleEntry, SupersedeError, SupersedeReason, SupersedesEdge,
};
pub use tree::{Entry, Tree};
