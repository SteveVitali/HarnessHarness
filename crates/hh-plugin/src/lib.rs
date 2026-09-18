//! `hh-plugin` — the C0/Stage-1 extensibility / plugin-architecture contract
//! surface (spec §8.4, R-2.12.2; ticket S1.27; ADR-0180…0182).
//!
//! What lives here (the kernel's plugin machinery that is *not* the manifest
//! record itself):
//!
//! - [`contract`] — `ContractRef`, `ContractVersionPolicy`, `check_compatibility`,
//!   `negotiate_version`, the kernel contract-policy table, and the
//!   contract-defining-tier table (ADR-0180 D3; ADR-0182 D1/D2).
//! - [`dag`] — `spec_dag_check(sections, class_records, plugins)`: the §4.4
//!   tables parsed as data, the graph checked for X2 tier monotonicity +
//!   acyclicity via `hh_ontology::dag` (AC-8; T-LCD-06's generalisation).
//! - [`lint`] — the module-graph lint for first-party in-process variants
//!   (X1's in-crate form; AC-9).
//! - [`corpus`] — the golden-manifest corpus driver (the naive second
//!   implementation — the registry supplies the checker).
//!
//! The `PluginManifest/1` codec + `admit_plugin`/`resolve_plugin_candidate`
//! live in `hh_registry::extension::plugin` (the manifest is an
//! `ExtensionRecord{kind: plugin}` payload — its record home is the extension
//! store, and `registry/1`'s closed `RecordKind` list must not grow — a
//! dedicated `contract_version_policy` record *kind* would be a `registry/2`
//! bump, so the policies ride on `RegistryPolicy` — ADR-0262). The
//! `plugin_abi/1` schema lives in `hh_embed_schema::plugin_abi` (one schema
//! source — V6/CC7).
//!
//! Design rules this crate encodes:
//! - Compatibility is checked inside `resolve`, **before** any executable is
//!   fetched, unpacked or launched (AC-2's ordering; §8.4 §2).
//! - Guard verdicts never include `allow` — that sum lives in the
//!   `plugin_abi/1` schema (no `allow` member, by construction).
//! - Nothing here references the Hosting ABI (`hh-hosting/1` is a different
//!   binding — CF-208; a `requires`/`depends_on` naming it fails as unknown
//!   contract, and as a tier violation on a C0/C1 depender — AC-12).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod contract;
pub mod corpus;
pub mod dag;
pub mod lint;

pub use contract::{
    check_compatibility, contract_defining_tier, kernel_contract_policies, negotiate_version,
    range_covers, version_cmp, ClassContract, ContractIncompatible, ContractRef, ContractRefError,
    ContractRefKind, ContractVersionPolicy, EvalPoint, Negotiation, SunsetBound, VersionRange,
    CURRENT_STAGE,
};
pub use corpus::{enumerate, run_corpus, CorpusCase, CorpusFailure};
pub use dag::{
    parse_spec_tables, spec_dag_check, ClassDagInput, ParsedSpecDag, PluginDagInput, SpecDagReport,
    KS_NODE,
};
pub use lint::{lint_module_source, lint_workspace, LintReport, LintRule, LintViolation};
