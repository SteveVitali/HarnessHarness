//! `hh-assembly` — the **configuration & composition model** (spec §3.3; ticket S1.9;
//! R-2.1.4 C0/Stage 1).
//!
//! The assembly grammar lives in the `hir/1` document's `assembly` member
//! ([`grammar`]): `Assembly{dialect, profile_binding, slots, parameters, values,
//! entities, constraints, layers?, resolved?, ext}` — data only, never host-language
//! content (Q-L1-09). [`schema`] declares the per-field merge policy and the
//! byte-level `encode`/`load` codec pair; [`load`] is the §3.3.4 document-level `load`
//! op. The op set: `validate_assembly` stages 1–5 + 7 with complete, never-fail-fast
//! diagnostic reports ([`validate`]), `resolve` — the one resolver feeding `hh-hir`'s
//! `seal` (CF-056) — `identity` ([`identity`]), `diff` via `HirDiff` ([`diff`], not a
//! separate object), in-process `instantiate` with `lifecycle.component.bound`
//! ([`instantiate`]), `verify_resume` with `lifecycle.definition.changed`
//! ([`resume`]), and `link_precheck` — the `C-LINK-1` refusal the compiler's stage 1
//! applies (AC-CC-10). The closed `AssemblyDiagnostic` taxonomy is [`diagnostics`]
//! (ADR-0148 + the ADR-0143 L2 code; `C-INT-1` is reserved — an uncoded rejection is a
//! defect).
//!
//! Reuse (CC1/CC7): identity through `hh-identity`'s `idp/1`, records through
//! `hh-registry`'s `ClassRecord`/`VariantRecord` and the one `RegistryStore`, the HIR
//! document/records/`seal`/`HirDiff` through `hh-hir`, provenance and the opacity
//! report through `hh-provenance`, the single fenced writer through `hh-ledger`, the
//! `ResourceAccount` input through `hh-budget`.
//!
//! Staged later (§3.3.13): `compose` with layers + `authority_cap` (Stage 3),
//! `space`/`enumerate` + the override grammar (Stage 3), stage 6a/6b (C1), the
//! out-of-process variant host (Stage 2 — DF-S1.9-1), the assembly service (§6).

pub mod boundary;
pub mod catalog;
pub mod diagnostics;
pub mod diff;
pub mod events;
pub mod grammar;
pub mod identity;
pub mod instantiate;
pub mod load;
pub mod resolve;
pub mod resume;
pub mod schema;
pub mod validate;

pub use boundary::{LadderRung, MUST_BE_CODE, MUST_BE_DATA, RUNG};
pub use catalog::{ClassCatalog, RegistryCatalog, Stage1Catalog, STAGE1_CLASSES};
pub use diagnostics::{
    detail_text, diagnostic_from_json, diagnostic_json, kern_code, AssemblyDiagnostic, Code,
    DerivedResults, NaReason, OpacitySummary, ReportStatus, Severity, Stage, StageOutcome,
    ValidationReport,
};
pub use diff::diff;
pub use events::{
    component_bound, definition_changed, emit, AssemblyEvent, ARTEFACT_DELIVERED, COMPONENT_BOUND,
    DEFINITION_CHANGED,
};
pub use grammar::{
    markers_in_json, Assembly, Constraint, ConstraintKind, EntityBinding, LayerProvenance,
    LayerSourceKind, ParamRequirement, ParamType, ParameterSpec, ProfileBinding, ResolvedInfo,
    ASSEMBLY_DIALECT, ENTITY_MARKER, PARAM_MARKER, SECRET_MARKER,
};
pub use identity::{
    assembly_identity_view, configuration, identity, CompositionInputs, ConfigurationIds,
};
pub use instantiate::{
    instantiate, BindContext, BindFailure, BindReason, BoundComponent, HarnessInstance,
    InstantiateOutcome, VariantRuntime,
};
pub use load::{load, LoadedDefinition};
pub use resolve::{entity_ref, link_precheck, resolve, resolved_info, ResolveEnv};
pub use resume::{
    verify_resume, AllowAll, BudgetGate, DenyAll, LedgerView, ResumeReason, ResumeRule,
    ResumeVerdict,
};
pub use schema::{encode, merge_policy, MergePolicy, MERGE_POLICIES};
pub use validate::{
    benchmark_hits, materialise, status_of, validate_assembly, ProfileView, Subject, BENCH_TOKENS,
};
