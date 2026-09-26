//! `PluginManifest/1` + plugin admission — the §8.4 (R-2.12.2) Stage-1 slice
//! (ADR-0180 D1/D3; ADR-0181 D2; ADR-0182 D1–D2; ADR-0262).
//!
//! A **plugin** is an `ExtensionRecord{kind: plugin}` whose `manifest` member
//! carries a `PluginManifest/1` — *a data document* (canonical `idp/1` form):
//! an inventory of `registry/1`-kind contributions, the Core contract versions
//! it `requires`, and the grants it `requests` (deny-by-default claims —
//! `requests` are lowered to a `Permission`/`ContainmentPolicy` only at `seal`
//! by a principal; `claims` never reach the monitor).
//!
//! Admission order (§8.4 §2 — every failure is raised **before any executable
//! is fetched, unpacked or launched**):
//!
//! 1. decode — strict member checking (an unknown non-`ext` key is
//!    `SchemaViolation`; an unknown contribution `kind` is
//!    `UnknownRecordKind`; an inline body, a serialized closure or a package-
//!    escaping `path_or_locator` is refused — AC-1);
//! 2. pinning — `executable.code_pointer` must be a `ContentAddress`, every
//!    `path_or_locator` locator and every `requires.records[]` ref must be
//!    pinned (`UnpinnedInSealedForm`);
//! 3. locality — `placement = in_process` is admissible only for `hh/`
//!    first-party plugins; `host_unconfined` is inadmissible for anything the
//!    kernel invokes; `component_model` is reserved (`LocalityInadmissible`);
//! 4. tier — every `depends_on` `ContractRef` names a contract whose
//!    *defining* tier ≤ the manifest's `tier` (`TierViolation`);
//! 5. compatibility — `check_compatibility` over `requires` (the dialect/ABI
//!    ranges materialised as `ContractRef`s + `requires.contracts`) and every
//!    contribution `contract_range` against the kernel policy table ∪
//!    `RegistryPolicy.contract_version_policies` + the registered class
//!    catalog (`ContractIncompatible{…, sunset?}`, complete report);
//! 6. requests — `requests ⊆ requests_cap` when the caller supplies the
//!    definition's ceiling (`RequestsExceedCap`); without a cap, requests are
//!    recorded claims only.
//!
//! Nothing in this module references the Hosting ABI: a `ContractRef` naming
//! `hh-hosting/1` finds no policy and fails `ContractIncompatible`; a
//! `depends_on` naming it is additionally a tier violation on a C0/C1
//! manifest (AC-12).

use std::collections::BTreeMap;

use hh_identity::idp::ContentAddress;
use hh_plugin::contract::{
    check_compatibility, contract_defining_tier, ClassContract, ContractIncompatible, ContractRef,
    EvalPoint, Negotiation,
};
use hh_plugin::dag::PluginDagInput;
use hh_provenance::AuthorityClass;
use hh_wire::json::Json;

use crate::kinds::Placement;
use crate::records::RegistryRecord;
use crate::store::RegistryStore;
use crate::RegistryError;

use super::{ExtensionKind, IsolationClass, SourceLocator};

// ── ManifestError ─────────────────────────────────────────────────────────────

/// The closed plugin-admission failure sum (§8.4 §2's typed pipeline errors
/// plus the manifest-level refusals). `kind_str` is the stable spelling the
/// golden corpus's `*.expect` files pin.
#[derive(Debug, Clone, PartialEq)]
pub enum ManifestError {
    /// The payload is not a JSON object.
    NotJson,
    /// The manifest is not schema-valid (`{path}` names the member — an
    /// unknown non-`ext` key, a wrong type, an inline body, a package-escaping
    /// path, a `summary` above `external`, a `conformance_claims` member that
    /// is not `publisher_claim`, …).
    SchemaViolation {
        /// The member path.
        path: String,
        /// What was wrong.
        detail: String,
    },
    /// A contribution `kind` outside the closed set (`UnknownRecordKind` —
    /// growth is a manifest dialect bump, never a silent skip).
    UnknownRecordKind {
        /// The spelling that failed.
        kind: String,
    },
    /// A `code_pointer`/locator/record ref that is not a pin (a selector or
    /// digest-less locator surviving into resolved form).
    UnpinnedInSealedForm {
        /// The detail.
        detail: String,
    },
    /// A `requires`/contribution `contract_range` misses every
    /// `ContractVersionPolicy` — the **complete** incompatibility report, in
    /// declaration order (a sunset refusal carries its dated bound).
    ContractIncompatible {
        /// Every incompatibility found.
        failures: Vec<ContractIncompatible>,
    },
    /// `placement_preference`/`isolation` inadmissible for this plugin's
    /// provenance (`in_process` is `hh/` first-party only; `host_unconfined`
    /// and the reserved `component_model` never admit).
    LocalityInadmissible {
        /// The plugin identity (`namespace/name`).
        origin: String,
        /// The refused placement/isolation.
        placement: String,
    },
    /// A `depends_on` ref whose defining tier exceeds the manifest's `tier`
    /// (X2 — upward reach refused at admission).
    TierViolation {
        /// The detail (`contract label: defined Cn > declared Cm`).
        detail: String,
    },
    /// `requests` exceeds the supplied ceiling (`authority_cap`-side check —
    /// claims are never grants).
    RequestsExceedCap {
        /// The detail.
        detail: String,
    },
    /// `identity.namespace` outside the Stage-1 `{hh, local}` set.
    NamespaceForbidden {
        /// The namespace.
        namespace: String,
    },
}

impl ManifestError {
    /// The stable kind spelling (the corpus `*.expect` vocabulary).
    pub fn kind_str(&self) -> &'static str {
        match self {
            ManifestError::NotJson => "NotJson",
            ManifestError::SchemaViolation { .. } => "SchemaViolation",
            ManifestError::UnknownRecordKind { .. } => "UnknownRecordKind",
            ManifestError::UnpinnedInSealedForm { .. } => "UnpinnedInSealedForm",
            ManifestError::ContractIncompatible { .. } => "ContractIncompatible",
            ManifestError::LocalityInadmissible { .. } => "LocalityInadmissible",
            ManifestError::TierViolation { .. } => "TierViolation",
            ManifestError::RequestsExceedCap { .. } => "RequestsExceedCap",
            ManifestError::NamespaceForbidden { .. } => "NamespaceForbidden",
        }
    }

    /// Map onto the registry's closed error sum (the store path) — kind names
    /// are preserved in `detail`/`path` text (the diagnostic's `reason` is the
    /// mapped variant's; the plugin kind survives in the detail string).
    pub fn into_registry_error(self) -> RegistryError {
        match self {
            ManifestError::NotJson => RegistryError::SchemaViolation {
                path: "manifest".to_string(),
                detail: "NotJson: manifest is not a JSON object".to_string(),
            },
            ManifestError::SchemaViolation { path, detail } => {
                RegistryError::SchemaViolation { path, detail }
            }
            ManifestError::UnknownRecordKind { kind } => RegistryError::UnknownRecordKind { kind },
            ManifestError::UnpinnedInSealedForm { detail } => RegistryError::SchemaViolation {
                path: "manifest".to_string(),
                detail: format!("UnpinnedInSealedForm: {detail}"),
            },
            ManifestError::ContractIncompatible { failures } => {
                RegistryError::ContractIncompatible {
                    detail: failures
                        .iter()
                        .map(|f| f.detail())
                        .collect::<Vec<_>>()
                        .join("; "),
                }
            }
            ManifestError::LocalityInadmissible { origin, placement } => {
                RegistryError::LocalityInadmissible { origin, placement }
            }
            ManifestError::TierViolation { detail } => RegistryError::ContractIncompatible {
                detail: format!("TierViolation: {detail}"),
            },
            ManifestError::RequestsExceedCap { detail } => RegistryError::SchemaViolation {
                path: "manifest.requests".to_string(),
                detail: format!("RequestsExceedCap: {detail}"),
            },
            ManifestError::NamespaceForbidden { namespace } => {
                RegistryError::NamespaceForbidden { namespace }
            }
        }
    }
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind_str())
    }
}

impl std::error::Error for ManifestError {}

// ── manifest types ────────────────────────────────────────────────────────────

/// `identity{namespace, name, version_label?}` — the plugin's semantic
/// coordinate (`identity is content; version_label is a claim`).
#[derive(Debug, Clone, PartialEq)]
pub struct PluginIdentity {
    /// The registry namespace (`hh/` first-party only for `in_process`).
    pub namespace: String,
    /// The plugin name.
    pub name: String,
    /// The SemVer-class label — a claim, never identity.
    pub version_label: Option<String>,
}

impl PluginIdentity {
    /// `namespace/name`.
    pub fn id(&self) -> String {
        format!("{}/{}", self.namespace, self.name)
    }
}

/// A `requires.records[]` member — a **pinned** record ref (`{version_id,
/// kind?}` — the manifest names coordinates; a selector/name-only form is a
/// claim, `UnpinnedInSealedForm`).
#[derive(Debug, Clone, PartialEq)]
pub struct PinnedRecordRef {
    /// The pinned `version_id`.
    pub version_id: String,
    /// The record kind spelling, when declared.
    pub kind: Option<String>,
}

/// `requires{hir_dialect, registry_dialect, plugin_abi, contracts, records}` —
/// what the plugin needs from the Core. The three dialect/binding ranges are
/// materialised as `ContractRef`s at compatibility-check time (CC1 — one
/// `ContractRef` form inside `check_compatibility`).
#[derive(Debug, Clone, PartialEq)]
pub struct Requires {
    /// The required `hir` dialect range.
    pub hir_dialect: String,
    /// The required `registry` dialect range.
    pub registry_dialect: String,
    /// The required `plugin_abi` range.
    pub plugin_abi: String,
    /// The consumed contracts (`ContractRef` — the only dependency form, X1).
    pub contracts: Vec<ContractRef>,
    /// The pinned records the plugin reads.
    pub records: Vec<PinnedRecordRef>,
}

/// One `requests.effects[]` member — `{domain, scope}`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EffectRequest {
    /// The effect domain.
    pub domain: String,
    /// The requested scope.
    pub scope: String,
}

/// `requests{effects[], fs_roots[], egress[], env_keys[], budget_share?,
/// hot_path_operations[]}` — **claims**, lowered to `Permission` +
/// `ContainmentPolicy` only at `seal` by a principal (deny-by-default;
/// ADR-0180 D2). Carried opaquely where the payload is another section's
/// (`egress` destinations, `budget_share`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Requests {
    /// Requested effect grants `{domain, scope}`.
    pub effects: Vec<EffectRequest>,
    /// Requested filesystem roots.
    pub fs_roots: Vec<String>,
    /// Requested egress destinations (opaque `Destination` payloads).
    pub egress: Vec<Json>,
    /// Requested environment keys.
    pub env_keys: Vec<String>,
    /// Requested budget share (opaque `BudgetSpec` payload).
    pub budget_share: Option<Json>,
    /// Operations the plugin asks to run on the hot path.
    pub hot_path_operations: Vec<String>,
}

impl Requests {
    /// `self ⊆ cap` — the `authority_cap`-side check (`RequestsExceedCap`).
    /// `None` cap at admit-time means "claims only — nothing conferred"; a
    /// supplied cap is the ceiling the sealing principal narrowed to.
    pub fn within_cap(&self, cap: &Requests) -> Result<(), ManifestError> {
        let exceed = |detail: String| ManifestError::RequestsExceedCap { detail };
        for e in &self.effects {
            if !cap.effects.contains(e) {
                return Err(exceed(format!(
                    "effect {{{},{}}} not under the cap",
                    e.domain, e.scope
                )));
            }
        }
        for r in &self.fs_roots {
            if !cap.fs_roots.contains(r) {
                return Err(exceed(format!("fs_root {r} not under the cap")));
            }
        }
        for d in &self.egress {
            if !cap.egress.contains(d) {
                return Err(exceed("egress destination not under the cap".to_string()));
            }
        }
        for k in &self.env_keys {
            if !cap.env_keys.contains(k) {
                return Err(exceed(format!("env_key {k} not under the cap")));
            }
        }
        if self.budget_share.is_some() && cap.budget_share.is_none() {
            return Err(exceed("budget_share requested with no cap".to_string()));
        }
        for op in &self.hot_path_operations {
            if !cap.hot_path_operations.contains(op) {
                return Err(exceed(format!("hot_path_operation {op} not under the cap")));
            }
        }
        Ok(())
    }
}

/// The contribution `kind` sum (§8.4 §3 — the `RecordKind`s a plugin may
/// contribute; unknown kinds are `UnknownRecordKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContributionKind {
    /// A `variant` record.
    Variant,
    /// A `capability` record.
    Capability,
    /// An `mcp_server` descriptor.
    McpServer,
    /// A `procedure_profile` (skill).
    ProcedureProfile,
    /// A `hook` (a `HarnessRule` with a code-pointer action).
    Hook,
    /// A `validator` record.
    Validator,
    /// A `model_profile` record.
    ModelProfile,
    /// A `surface_family` record.
    SurfaceFamily,
    /// A `benchmark_adapter` record.
    BenchmarkAdapter,
    /// An `environment_family` record.
    EnvironmentFamily,
    /// A `metric_declaration` record.
    MetricDeclaration,
    /// An `instruction_file` record.
    InstructionFile,
    /// A `harness_rule` record.
    HarnessRule,
}

impl ContributionKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ContributionKind::Variant => "variant",
            ContributionKind::Capability => "capability",
            ContributionKind::McpServer => "mcp_server",
            ContributionKind::ProcedureProfile => "procedure_profile",
            ContributionKind::Hook => "hook",
            ContributionKind::Validator => "validator",
            ContributionKind::ModelProfile => "model_profile",
            ContributionKind::SurfaceFamily => "surface_family",
            ContributionKind::BenchmarkAdapter => "benchmark_adapter",
            ContributionKind::EnvironmentFamily => "environment_family",
            ContributionKind::MetricDeclaration => "metric_declaration",
            ContributionKind::InstructionFile => "instruction_file",
            ContributionKind::HarnessRule => "harness_rule",
        }
    }

    /// Parse a spelling; `None` for anything outside the closed sum.
    pub fn parse(s: &str) -> Option<ContributionKind> {
        Some(match s {
            "variant" => ContributionKind::Variant,
            "capability" => ContributionKind::Capability,
            "mcp_server" => ContributionKind::McpServer,
            "procedure_profile" => ContributionKind::ProcedureProfile,
            "hook" => ContributionKind::Hook,
            "validator" => ContributionKind::Validator,
            "model_profile" => ContributionKind::ModelProfile,
            "surface_family" => ContributionKind::SurfaceFamily,
            "benchmark_adapter" => ContributionKind::BenchmarkAdapter,
            "environment_family" => ContributionKind::EnvironmentFamily,
            "metric_declaration" => ContributionKind::MetricDeclaration,
            "instruction_file" => ContributionKind::InstructionFile,
            "harness_rule" => ContributionKind::HarnessRule,
            _ => return None,
        })
    }
}

/// `path_or_locator` — a package-relative path, or a pinned locator object.
#[derive(Debug, Clone, PartialEq)]
pub enum PathOrLocator {
    /// A package-relative path (never escapes the package root — `..`,
    /// absolute paths and drive spellings are refused at decode).
    PackagePath(String),
    /// A pinned source locator (selector consumed, `resolved` + `fetched_at`
    /// present — `UnpinnedInSealedForm` otherwise).
    PinnedLocator(SourceLocator),
}

/// `executable{code_pointer, isolation, placement_preference?, one_shot?,
/// host_requirements}` — the contribution member carrying runnable content.
/// `code_pointer` is a `ContentAddress` after `resolve` — the pin, never the
/// bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct Executable {
    /// The pinned executable content (`ContentAddress`).
    pub code_pointer: ContentAddress,
    /// The declared isolation class (`host_unconfined` is inadmissible for
    /// kernel-invoked contributions — §8.4 §3 Placement row).
    pub isolation: IsolationClass,
    /// The requested placement (`in_process` is `hh/` first-party only).
    pub placement_preference: Option<Placement>,
    /// The `one_shot` hint (spawn-per-invocation vs persistent session —
    /// OQ-408 default deferred; the flag is carried, never interpreted here).
    pub one_shot: Option<bool>,
    /// Host requirements (opaque `TypedValue` payload — checked by the
    /// variant host at `hello`, carried verbatim here).
    pub host_requirements: Json,
}

/// `Contribution{kind, path_or_locator, declaration, executable?,
/// contract_range?}` — one `registry/1` record the plugin contributes (its
/// own `version_id`, envelope and — when executable — trust legs).
#[derive(Debug, Clone, PartialEq)]
pub struct Contribution {
    /// The contribution kind (closed sum).
    pub kind: ContributionKind,
    /// Where the contributed record body lives (package path or pinned
    /// locator — never an inline body).
    pub path_or_locator: PathOrLocator,
    /// The record declaration (opaque `TypedValue` payload — the record's own
    /// schema validates it at contribution registration; a `variant`
    /// contribution carrying `contract_range` must name `class_id` here).
    pub declaration: Json,
    /// The executable member, when the contribution is runnable.
    pub executable: Option<Executable>,
    /// The contract-version range the contribution implements.
    pub contract_range: Option<String>,
}

/// `PluginManifest/1` — the §8.4 §3 inventory document. A data document:
/// canonical `idp/1` form, content-addressed over the package, carrying no
/// code, closures or host-language content (ADR-0023 D1/ADR-0024 pointer
/// rule). `ext` never decides admission, authority or compatibility.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginManifest {
    /// `identity{namespace, name, version_label?}`.
    pub identity: PluginIdentity,
    /// The manifest's declared `tier` (`C0|C1|C2|C3|C4` — ADR-0182 D1).
    pub tier: String,
    /// The manifest's `depends_on` contracts (X1 form).
    pub depends_on: Vec<ContractRef>,
    /// `requires{…}` — what the plugin needs from the Core.
    pub requires: Requires,
    /// `contributions[]` — the plugin's behaviour is exactly the closure of
    /// its contributions.
    pub contributions: Vec<Contribution>,
    /// `requests{…}` — claims, never grants.
    pub requests: Requests,
    /// `claims` — an opaque `TypedValue` payload that never reaches the
    /// monitor (ADR-0063 L3).
    pub claims: Json,
    /// `conformance_claims[]` — `produced_by = publisher_claim` only (they
    /// never count toward `probed`).
    pub conformance_claims: Vec<Json>,
    /// `parameters?: map<param_id, ParameterSpec>` — opaque payloads.
    pub parameters: Option<BTreeMap<String, Json>>,
    /// `summary: Text{authority ≤ external}` — never model-facing.
    pub summary: Json,
    /// The `ext` escape-hatch (never admission-relevant).
    pub ext: BTreeMap<String, Json>,
}

impl PluginManifest {
    /// The DAG view (`plugin:<ns>/<name>` at `tier`; `depends_on` ∪
    /// `requires` contracts are the edges — X1 makes them the only reach).
    pub fn dag_input(&self) -> PluginDagInput {
        let mut depends_on = self.depends_on.clone();
        depends_on.extend(self.requires.contracts.iter().cloned());
        PluginDagInput {
            plugin_id: self.identity.id(),
            tier: parse_tier(&self.tier).unwrap_or(0),
            depends_on,
        }
    }

    /// Whether this manifest is `hh/`-namespaced (the first-party half of the
    /// `in_process` admissibility rule — the registrar-authority half is the
    /// caller's [`AdmissionContext`]).
    pub fn is_kernel_namespace(&self) -> bool {
        self.identity.namespace == "hh" || self.identity.namespace.starts_with("hh/")
    }
}

/// `tier` spelling → the DAG digit (`C0`→0 … `C4`→4).
pub fn parse_tier(tier: &str) -> Option<u8> {
    tier.strip_prefix('C')?
        .parse::<u8>()
        .ok()
        .filter(|n| *n <= 4)
}

// ── admission ─────────────────────────────────────────────────────────────────

/// The context `admit_plugin` needs beyond the manifest: the registrar's
/// first-party status (conferred authority — never read from the manifest,
/// CC2), the sunset evaluation point, and the optional `requests` ceiling.
#[derive(Debug, Clone)]
pub struct AdmissionContext {
    /// Whether the registrar is first-party (`kernel`/`definition`
    /// authority). Combined with `identity.namespace ⊆ hh/` for the
    /// `in_process` admissibility rule.
    pub first_party: bool,
    /// The sunset evaluation point (`EvalPoint::kernel()` inside the store —
    /// the store carries no wall clock).
    pub now: EvalPoint,
    /// The definition's `requests` ceiling (the `authority_cap`-side bound);
    /// `None` = claims recorded, cap check deferred to `seal`/`validate`.
    pub requests_cap: Option<Requests>,
}

/// The admission report — what `resolve` records for a passing manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct AdmissionReport {
    /// `namespace/name`.
    pub plugin_id: String,
    /// The negotiated contract versions (what `hello` records at Stage 2).
    pub negotiated: Negotiation,
    /// One line per admitted contribution (`<kind>:<path|locator>` +
    /// negotiated `contract_range` version when the contribution carries one).
    pub contributions: Vec<String>,
    /// The effective placement per executable contribution (`declared
    /// preference`, or the default: `subprocess_confined` for third-party,
    /// `in_process` only where admissible).
    pub placements: Vec<Placement>,
}

/// The class catalog as `check_compatibility` consumes it: every registered
/// `ClassRecord` → `ClassContract{class_id, contract_version, tier,
/// decision_points}`.
pub fn class_catalog(store: &RegistryStore) -> Vec<ClassContract> {
    let mut out = Vec::new();
    for vid in store.version_ids() {
        if let Some((_, RegistryRecord::Class(c))) = store.get(vid) {
            out.push(ClassContract {
                class_id: c.class_id.clone(),
                contract_version: c.contract_version.clone(),
                tier: parse_tier(&c.tier).unwrap_or(0),
                decision_points: c.decision_points.clone(),
            });
        }
    }
    out
}

/// The policies `check_compatibility` runs against:
/// `RegistryPolicy.contract_version_policies` ∪ the kernel contract table ∪ a
/// `record_kind:<kind>` policy per `registry/1` `RecordKind` (supported
/// `["1"]` — the record-kind contract version is the dialect's). **Layered
/// first**, so an operator policy can narrow the built-ins (a sunset on a
/// supported version) while the built-ins supply the contract set itself (a
/// dedicated policy record *kind* would be a `registry/2` bump; ADR-0262).
/// The record-kind rows are generated from `RecordKind::ALL` — the one closed
/// sum, never re-spelled here (CC1/CC7).
pub fn admission_policies(store: &RegistryStore) -> Vec<hh_plugin::ContractVersionPolicy> {
    let mut out = store.policy().contract_version_policies.clone();
    out.extend(hh_plugin::kernel_contract_policies());
    for k in crate::kinds::RecordKind::ALL {
        out.push(hh_plugin::ContractVersionPolicy {
            contract: ContractRef::record_kind(k.as_str(), "*"),
            supported: vec!["1".to_string()],
            sunset: std::collections::BTreeMap::new(),
            additive_only: true,
        });
    }
    out
}

/// The `requires` refs materialised: `hir_dialect`/`registry_dialect`/
/// `plugin_abi` ranges as `ContractRef`s (the dialect ids name the contract
/// line — `hir/1`, `registry/1`; the range is over the version) plus
/// `requires.contracts` verbatim plus each contribution's `contract_range`
/// (a `variant`'s range checks against the class contract its `declaration`
/// names; any other kind's range checks against `record_kind:<kind>` — a
/// contract no policy names is refused, never silently admitted).
pub fn requires_refs(manifest: &PluginManifest) -> Result<Vec<ContractRef>, ManifestError> {
    let mut refs = vec![
        ContractRef::dialect("hir/1", manifest.requires.hir_dialect.clone()),
        ContractRef::dialect("registry/1", manifest.requires.registry_dialect.clone()),
        ContractRef::protocol_binding("plugin_abi/1", manifest.requires.plugin_abi.clone()),
    ];
    refs.extend(manifest.requires.contracts.iter().cloned());
    for c in &manifest.contributions {
        if let Some(range) = &c.contract_range {
            if c.kind == ContributionKind::Variant {
                let class_id = c
                    .declaration
                    .get("class_id")
                    .and_then(Json::as_str)
                    .ok_or_else(|| ManifestError::SchemaViolation {
                        path: "contributions.declaration".to_string(),
                        detail: "a variant contribution with contract_range must name \
                                 declaration.class_id"
                            .to_string(),
                    })?;
                refs.push(ContractRef::class_contract(class_id, range.clone()));
            } else {
                refs.push(ContractRef::record_kind(c.kind.as_str(), range.clone()));
            }
        }
    }
    Ok(refs)
}

/// `admit_plugin(store, manifest, ctx)` — the §8.4 §2 admission gate: pin,
/// locality, tier, compatibility and (when a cap is supplied) requests checks,
/// **all before any executable is fetched or launched** (AC-2). The store is
/// read-only here — this function performs no I/O and launches nothing (CC5).
pub fn admit_plugin(
    store: &RegistryStore,
    manifest: &PluginManifest,
    ctx: &AdmissionContext,
) -> Result<AdmissionReport, ManifestError> {
    // Namespace discipline: Stage-1 namespaces are hh/local (the store's own
    // rule for NamespaceRecord applies to manifest identity — CC10).
    if !matches!(manifest.identity.namespace.as_str(), "hh" | "local")
        && !manifest.identity.namespace.starts_with("hh/")
        && !manifest.identity.namespace.starts_with("local/")
    {
        return Err(ManifestError::NamespaceForbidden {
            namespace: manifest.identity.namespace.clone(),
        });
    }
    let first_party = ctx.first_party && manifest.is_kernel_namespace();
    let plugin_id = manifest.identity.id();

    // Locality: executable contributions.
    for c in &manifest.contributions {
        if let Some(exe) = &c.executable {
            if exe.isolation == IsolationClass::HostUnconfined {
                return Err(ManifestError::LocalityInadmissible {
                    origin: plugin_id.clone(),
                    placement: exe.isolation.as_str().to_string(),
                });
            }
            match exe.placement_preference {
                Some(Placement::InProcess) if !first_party => {
                    return Err(ManifestError::LocalityInadmissible {
                        origin: plugin_id.clone(),
                        placement: Placement::InProcess.as_str().to_string(),
                    });
                }
                Some(Placement::ComponentModel) => {
                    return Err(ManifestError::LocalityInadmissible {
                        origin: plugin_id.clone(),
                        placement: Placement::ComponentModel.as_str().to_string(),
                    });
                }
                _ => {}
            }
        }
    }

    // Tier: every depends_on ref names a contract whose defining tier ≤ the
    // manifest's (X2). An unnameable depends_on is a compatibility failure —
    // a dependency reachable through no contract is refused, never ignored.
    let catalog = class_catalog(store);
    let policies = admission_policies(store);
    let tier = parse_tier(&manifest.tier).ok_or_else(|| ManifestError::SchemaViolation {
        path: "tier".to_string(),
        detail: format!("`{}` is not a tier spelling (C0..C4)", manifest.tier),
    })?;
    for r in &manifest.depends_on {
        match contract_defining_tier(r, &catalog) {
            Some(t) if t > tier => {
                return Err(ManifestError::TierViolation {
                    detail: format!("{} defined at C{t} > manifest tier C{tier}", r.label()),
                });
            }
            None => {
                return Err(ManifestError::ContractIncompatible {
                    failures: vec![ContractIncompatible {
                        contract: r.clone(),
                        offered_range: r.version_range.clone(),
                        supported_range: String::new(),
                        sunset: None,
                    }],
                });
            }
            _ => {}
        }
    }

    // Compatibility (AC-2's ordering: before fetch/launch).
    let refs = requires_refs(manifest)?;
    let negotiated = check_compatibility(&refs, &policies, &catalog, &ctx.now)
        .map_err(|failures| ManifestError::ContractIncompatible { failures })?;

    // Requests ⊆ cap when the caller supplies the definition's ceiling.
    if let Some(cap) = &ctx.requests_cap {
        manifest.requests.within_cap(cap)?;
    }

    let mut contributions = Vec::new();
    let mut placements = Vec::new();
    for c in &manifest.contributions {
        let loc = match &c.path_or_locator {
            PathOrLocator::PackagePath(p) => p.clone(),
            PathOrLocator::PinnedLocator(l) => l.credential_free_uri.clone(),
        };
        contributions.push(format!("{}:{loc}", c.kind.as_str()));
        if let Some(exe) = &c.executable {
            placements.push(exe.placement_preference.unwrap_or(if first_party {
                Placement::InProcess
            } else {
                Placement::SubprocessConfined
            }));
        }
    }

    Ok(AdmissionReport {
        plugin_id,
        negotiated,
        contributions,
        placements,
    })
}

/// `resolve_plugin_candidate(store, bytes, ctx)` — the candidate-side entry:
/// decode + admit in one step (the corpus/CLI form; an admitted candidate is
/// what `register` then stores as `ExtensionRecord{kind: plugin}`).
pub fn resolve_plugin_candidate(
    store: &RegistryStore,
    bytes: &[u8],
    ctx: &AdmissionContext,
) -> Result<AdmissionReport, ManifestError> {
    let manifest = manifest_from_bytes(bytes)?;
    admit_plugin(store, &manifest, ctx)
}

/// The execute-mode re-check inside `resolve` (§8.4 §2: "`resolve(mode =
/// execute)` refuses past the sunset with a dated reason"). Decodes the
/// manifest carried on an `ExtensionRecord{kind: plugin}` and re-runs
/// `check_compatibility` — a sunset that passed between admission and execute
/// refuses here, before launch.
pub fn execute_mode_compat_check(
    store: &RegistryStore,
    manifest_json: &Json,
) -> Result<(), RegistryError> {
    let manifest = manifest_from_json(manifest_json, "manifest")
        .map_err(ManifestError::into_registry_error)?;
    let catalog = class_catalog(store);
    let policies = admission_policies(store);
    let refs = requires_refs(&manifest).map_err(ManifestError::into_registry_error)?;
    check_compatibility(&refs, &policies, &catalog, &EvalPoint::kernel()).map_err(|failures| {
        ManifestError::ContractIncompatible { failures }.into_registry_error()
    })?;
    Ok(())
}

// ── codec ─────────────────────────────────────────────────────────────────────

fn bad(path: &str, detail: impl Into<String>) -> ManifestError {
    ManifestError::SchemaViolation {
        path: path.to_string(),
        detail: detail.into(),
    }
}

fn req<'a>(j: &'a Json, k: &str, path: &str) -> Result<&'a Json, ManifestError> {
    j.get(k)
        .ok_or_else(|| bad(path, format!("missing member `{k}`")))
}

fn req_str(j: &Json, k: &str, path: &str) -> Result<String, ManifestError> {
    req(j, k, path)?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| bad(path, format!("`{k}` is not a string")))
}

fn opt_str(j: &Json, k: &str) -> Option<String> {
    j.get(k).and_then(Json::as_str).map(str::to_string)
}

fn req_arr<'a>(j: &'a Json, k: &str, path: &str) -> Result<&'a Vec<Json>, ManifestError> {
    match req(j, k, path)? {
        Json::Arr(a) => Ok(a),
        _ => Err(bad(path, format!("`{k}` is not an array"))),
    }
}

fn str_vec(j: &Json, k: &str, path: &str) -> Result<Vec<String>, ManifestError> {
    req_arr(j, k, path)?
        .iter()
        .enumerate()
        .map(|(i, v)| {
            v.as_str()
                .map(str::to_string)
                .ok_or_else(|| bad(path, format!("`{k}[{i}]` is not a string")))
        })
        .collect()
}

/// Decode `PluginManifest/1` from raw bytes (`idp/1` canonical JSON).
pub fn manifest_from_bytes(bytes: &[u8]) -> Result<PluginManifest, ManifestError> {
    let s = std::str::from_utf8(bytes).map_err(|_| ManifestError::NotJson)?;
    let j = hh_wire::json::parse(s).map_err(|_| ManifestError::NotJson)?;
    manifest_from_json(&j, "manifest")
}

fn cref_from_json(j: &Json, path: &str) -> Result<ContractRef, ManifestError> {
    ContractRef::from_json(j, path).map_err(|e| match e {
        hh_plugin::ContractRefError::SchemaViolation { path, detail } => {
            ManifestError::SchemaViolation { path, detail }
        }
    })
}

/// Decode `PluginManifest/1` from a canonical `Json` member. Strict: every
/// unknown non-`ext` member is `SchemaViolation` (AC-1 — a manifest is
/// inventory, never a host-language carrier).
pub fn manifest_from_json(j: &Json, path: &str) -> Result<PluginManifest, ManifestError> {
    let Json::Obj(m) = j else {
        return Err(ManifestError::NotJson);
    };
    const KNOWN: &[&str] = &[
        "identity",
        "tier",
        "depends_on",
        "requires",
        "contributions",
        "requests",
        "claims",
        "conformance_claims",
        "parameters",
        "summary",
        "ext",
    ];
    for k in m.keys() {
        if !KNOWN.contains(&k.as_str()) {
            return Err(bad(path, format!("unknown non-`ext` member `{k}`")));
        }
    }

    // identity{namespace, name, version_label?}
    let ip = format!("{path}.identity");
    let identity = {
        let ij = req(j, "identity", path)?;
        let Json::Obj(im) = ij else {
            return Err(bad(&ip, "expected object"));
        };
        for k in im.keys() {
            if !matches!(k.as_str(), "namespace" | "name" | "version_label" | "ext") {
                return Err(bad(&ip, format!("unknown member `{k}`")));
            }
        }
        PluginIdentity {
            namespace: req_str(ij, "namespace", &ip)?,
            name: req_str(ij, "name", &ip)?,
            version_label: opt_str(ij, "version_label"),
        }
    };

    let tier = req_str(j, "tier", path)?;
    if parse_tier(&tier).is_none() {
        return Err(bad(
            path,
            format!("`tier` `{tier}` is not a C0..C4 spelling"),
        ));
    }

    let depends_on = req_arr(j, "depends_on", path)?
        .iter()
        .enumerate()
        .map(|(i, v)| cref_from_json(v, &format!("{path}.depends_on[{i}]")))
        .collect::<Result<Vec<_>, _>>()?;

    // requires{hir_dialect, registry_dialect, plugin_abi, contracts, records}
    let rp = format!("{path}.requires");
    let requires = {
        let rj = req(j, "requires", path)?;
        let Json::Obj(rm) = rj else {
            return Err(bad(&rp, "expected object"));
        };
        for k in rm.keys() {
            if !matches!(
                k.as_str(),
                "hir_dialect" | "registry_dialect" | "plugin_abi" | "contracts" | "records" | "ext"
            ) {
                return Err(bad(&rp, format!("unknown member `{k}`")));
            }
        }
        let contracts = req_arr(rj, "contracts", &rp)?
            .iter()
            .enumerate()
            .map(|(i, v)| cref_from_json(v, &format!("{rp}.contracts[{i}]")))
            .collect::<Result<Vec<_>, _>>()?;
        let mut records = Vec::new();
        for (i, v) in req_arr(rj, "records", &rp)?.iter().enumerate() {
            let p = format!("{rp}.records[{i}]");
            let Json::Obj(vm) = v else {
                return Err(bad(&p, "expected pinned record ref object"));
            };
            for k in vm.keys() {
                if !matches!(k.as_str(), "version_id" | "kind" | "ext") {
                    return Err(bad(
                        &p,
                        format!("unknown member `{k}` (a record ref is a pin, not a selector)"),
                    ));
                }
            }
            let version_id = req_str(v, "version_id", &p)?;
            if version_id.is_empty() || !version_id.contains(':') {
                return Err(ManifestError::UnpinnedInSealedForm {
                    detail: format!("{p}.version_id `{version_id}` is not a pinned id"),
                });
            }
            records.push(PinnedRecordRef {
                version_id,
                kind: opt_str(v, "kind"),
            });
        }
        Requires {
            hir_dialect: req_str(rj, "hir_dialect", &rp)?,
            registry_dialect: req_str(rj, "registry_dialect", &rp)?,
            plugin_abi: req_str(rj, "plugin_abi", &rp)?,
            contracts,
            records,
        }
    };

    // contributions[]
    let mut contributions = Vec::new();
    for (i, v) in req_arr(j, "contributions", path)?.iter().enumerate() {
        contributions.push(contribution_from_json(
            v,
            &format!("{path}.contributions[{i}]"),
        )?);
    }

    // requests{…} — absent or empty object = no requests (deny-by-default).
    let rp = format!("{path}.requests");
    let requests = match j.get("requests") {
        None => Requests::default(),
        Some(rj) => {
            let Json::Obj(rm) = rj else {
                return Err(bad(&rp, "expected object"));
            };
            for k in rm.keys() {
                if !matches!(
                    k.as_str(),
                    "effects"
                        | "fs_roots"
                        | "egress"
                        | "env_keys"
                        | "budget_share"
                        | "hot_path_operations"
                        | "ext"
                ) {
                    return Err(bad(&rp, format!("unknown member `{k}`")));
                }
            }
            let mut effects = Vec::new();
            for (i, v) in req_arr(rj, "effects", &rp)?.iter().enumerate() {
                let p = format!("{rp}.effects[{i}]");
                let Json::Obj(em) = v else {
                    return Err(bad(&p, "expected object"));
                };
                for k in em.keys() {
                    if !matches!(k.as_str(), "domain" | "scope" | "ext") {
                        return Err(bad(&p, format!("unknown member `{k}`")));
                    }
                }
                effects.push(EffectRequest {
                    domain: req_str(v, "domain", &p)?,
                    scope: req_str(v, "scope", &p)?,
                });
            }
            Requests {
                effects,
                fs_roots: str_vec(rj, "fs_roots", &rp)?,
                egress: req_arr(rj, "egress", &rp)?.clone(),
                env_keys: str_vec(rj, "env_keys", &rp)?,
                budget_share: rj.get("budget_share").cloned(),
                hot_path_operations: str_vec(rj, "hot_path_operations", &rp)?,
            }
        }
    };

    let claims = req(j, "claims", path)?.clone();
    if !matches!(claims, Json::Obj(_)) {
        return Err(bad(path, "`claims` is not an object"));
    }

    // conformance_claims[] — produced_by = publisher_claim only (they never
    // count toward `probed` — ADR-0152 D3).
    let mut conformance_claims = Vec::new();
    for (i, v) in req_arr(j, "conformance_claims", path)?.iter().enumerate() {
        let p = format!("{path}.conformance_claims[{i}]");
        match v.get("produced_by").and_then(Json::as_str) {
            Some("publisher_claim") => conformance_claims.push(v.clone()),
            Some(other) => {
                return Err(bad(
                    &p,
                    format!("produced_by `{other}` — manifest claims are `publisher_claim` only"),
                ));
            }
            None => return Err(bad(&p, "missing `produced_by`")),
        }
    }

    let parameters = match j.get("parameters") {
        None | Some(Json::Null) => None,
        Some(Json::Obj(pm)) => Some(pm.clone()),
        Some(_) => return Err(bad(path, "`parameters` is not an object")),
    };

    // summary — a Text leaf ≤ external (never model-facing; §06 R-NM).
    let sp = format!("{path}.summary");
    let summary = req(j, "summary", path)?.clone();
    let summary_text = hh_hir::leaves::Text::from_json(&summary, &sp)
        .map_err(|e| bad(&sp, format!("not a Text leaf: {e:?}")))?;
    if summary_text.authority > AuthorityClass::External {
        return Err(bad(
            &sp,
            format!(
                "authority {} exceeds `external` (a manifest never mints above external)",
                summary_text.authority.as_str()
            ),
        ));
    }

    let ext = match j.get("ext") {
        Some(Json::Obj(em)) => em.clone(),
        None => BTreeMap::new(),
        Some(_) => return Err(bad(path, "`ext` is not an object")),
    };

    Ok(PluginManifest {
        identity,
        tier,
        depends_on,
        requires,
        contributions,
        requests,
        claims,
        conformance_claims,
        parameters,
        summary,
        ext,
    })
}

/// Encode `PluginManifest/1` (canonical member order — sorted keys, matching
/// the registry's `obj()` convention).
pub fn manifest_to_json(m: &PluginManifest) -> Json {
    let mut id = BTreeMap::from([
        ("name".to_string(), Json::str(m.identity.name.clone())),
        (
            "namespace".to_string(),
            Json::str(m.identity.namespace.clone()),
        ),
    ]);
    if let Some(v) = &m.identity.version_label {
        id.insert("version_label".to_string(), Json::str(v.clone()));
    }

    let requires = Json::obj([
        (
            "contracts",
            Json::Arr(m.requires.contracts.iter().map(|c| c.to_json()).collect()),
        ),
        ("hir_dialect", Json::str(m.requires.hir_dialect.clone())),
        ("plugin_abi", Json::str(m.requires.plugin_abi.clone())),
        (
            "records",
            Json::Arr(
                m.requires
                    .records
                    .iter()
                    .map(|r| {
                        let mut o = BTreeMap::from([(
                            "version_id".to_string(),
                            Json::str(r.version_id.clone()),
                        )]);
                        if let Some(k) = &r.kind {
                            o.insert("kind".to_string(), Json::str(k.clone()));
                        }
                        Json::Obj(o)
                    })
                    .collect(),
            ),
        ),
        (
            "registry_dialect",
            Json::str(m.requires.registry_dialect.clone()),
        ),
    ]);

    let contributions = Json::Arr(
        m.contributions
            .iter()
            .map(|c| {
                let mut o = BTreeMap::new();
                if let Some(cr) = &c.contract_range {
                    o.insert("contract_range".to_string(), Json::str(cr.clone()));
                }
                o.insert("declaration".to_string(), c.declaration.clone());
                if let Some(e) = &c.executable {
                    let mut eo = BTreeMap::from([
                        (
                            "code_pointer".to_string(),
                            Json::obj([
                                ("algorithm", Json::str(e.code_pointer.algorithm)),
                                ("digest", Json::str(e.code_pointer.digest.clone())),
                                ("idp", Json::str(e.code_pointer.idp)),
                                ("media_type", Json::str(e.code_pointer.media_type.clone())),
                                ("size", Json::Int(e.code_pointer.size as i64)),
                            ]),
                        ),
                        ("isolation".to_string(), Json::str(e.isolation.as_str())),
                    ]);
                    if let Some(p) = e.placement_preference {
                        eo.insert("placement_preference".to_string(), Json::str(p.as_str()));
                    }
                    if let Some(s) = e.one_shot {
                        eo.insert("one_shot".to_string(), Json::Bool(s));
                    }
                    eo.insert("host_requirements".to_string(), e.host_requirements.clone());
                    o.insert("executable".to_string(), Json::Obj(eo));
                }
                o.insert("kind".to_string(), Json::str(c.kind.as_str()));
                o.insert(
                    "path_or_locator".to_string(),
                    match &c.path_or_locator {
                        PathOrLocator::PackagePath(p) => Json::str(p.clone()),
                        PathOrLocator::PinnedLocator(l) => Json::obj([
                            (
                                "credential_free_uri",
                                Json::str(l.credential_free_uri.clone()),
                            ),
                            (
                                "fetched_at",
                                l.fetched_at.map_or(Json::Null, |t| Json::Int(t as i64)),
                            ),
                            ("resolved", l.resolved.clone().map_or(Json::Null, Json::Str)),
                            ("scheme", Json::str(l.scheme.clone())),
                            ("selector", l.selector.clone().map_or(Json::Null, Json::Str)),
                        ]),
                    },
                );
                Json::Obj(o)
            })
            .collect(),
    );

    let requests = {
        let mut r = BTreeMap::from([
            (
                "effects".to_string(),
                Json::Arr(
                    m.requests
                        .effects
                        .iter()
                        .map(|e| {
                            Json::obj([
                                ("domain", Json::str(e.domain.clone())),
                                ("scope", Json::str(e.scope.clone())),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("egress".to_string(), Json::Arr(m.requests.egress.clone())),
            (
                "env_keys".to_string(),
                Json::Arr(
                    m.requests
                        .env_keys
                        .iter()
                        .map(|k| Json::str(k.clone()))
                        .collect(),
                ),
            ),
            (
                "fs_roots".to_string(),
                Json::Arr(
                    m.requests
                        .fs_roots
                        .iter()
                        .map(|r| Json::str(r.clone()))
                        .collect(),
                ),
            ),
            (
                "hot_path_operations".to_string(),
                Json::Arr(
                    m.requests
                        .hot_path_operations
                        .iter()
                        .map(|o| Json::str(o.clone()))
                        .collect(),
                ),
            ),
        ]);
        if let Some(b) = &m.requests.budget_share {
            r.insert("budget_share".to_string(), b.clone());
        }
        Json::Obj(r)
    };

    let mut o = BTreeMap::from([
        ("claims".to_string(), m.claims.clone()),
        (
            "conformance_claims".to_string(),
            Json::Arr(m.conformance_claims.clone()),
        ),
        ("contributions".to_string(), contributions),
        (
            "depends_on".to_string(),
            Json::Arr(m.depends_on.iter().map(|c| c.to_json()).collect()),
        ),
        ("identity".to_string(), Json::Obj(id)),
        ("requests".to_string(), requests),
        ("requires".to_string(), requires),
        ("summary".to_string(), m.summary.clone()),
        ("tier".to_string(), Json::str(m.tier.clone())),
    ]);
    if let Some(p) = &m.parameters {
        o.insert("parameters".to_string(), Json::Obj(p.clone()));
    }
    if !m.ext.is_empty() {
        o.insert("ext".to_string(), Json::Obj(m.ext.clone()));
    }
    Json::Obj(o)
}

/// A package-relative path check: non-empty, no `..`, no leading `/` or `\`,
/// no drive spelling, no NUL (AC-1 — a path outside the package root is a
/// refusal).
fn check_package_path(p: &str, path: &str) -> Result<PathOrLocator, ManifestError> {
    let outside = p.is_empty()
        || p.starts_with('/')
        || p.starts_with('\\')
        || p.contains('\0')
        || p.contains(':') // drive spellings (`C:…`) and URI-ish members are not package paths
        || p.split('/').chain(p.split('\\')).any(|seg| seg == "..");
    if outside {
        return Err(bad(
            path,
            format!("path `{p}` escapes the package root (a manifest names package members only)"),
        ));
    }
    Ok(PathOrLocator::PackagePath(p.to_string()))
}

fn contribution_from_json(j: &Json, path: &str) -> Result<Contribution, ManifestError> {
    let Json::Obj(m) = j else {
        return Err(bad(path, "expected object"));
    };
    for k in m.keys() {
        if !matches!(
            k.as_str(),
            "kind" | "path_or_locator" | "declaration" | "executable" | "contract_range" | "ext"
        ) {
            return Err(bad(path, format!("unknown member `{k}`")));
        }
    }
    let kind_s = req_str(j, "kind", path)?;
    let kind = ContributionKind::parse(&kind_s)
        .ok_or(ManifestError::UnknownRecordKind { kind: kind_s })?;

    let path_or_locator = match req(j, "path_or_locator", path)? {
        Json::Str(p) => check_package_path(p, &format!("{path}.path_or_locator"))?,
        Json::Obj(lm) => {
            let lp = format!("{path}.path_or_locator");
            for k in lm.keys() {
                if !matches!(
                    k.as_str(),
                    "scheme"
                        | "credential_free_uri"
                        | "selector"
                        | "resolved"
                        | "fetched_at"
                        | "ext"
                ) {
                    return Err(bad(&lp, format!("unknown member `{k}`")));
                }
            }
            let loc = SourceLocator {
                scheme: req_str(req(j, "path_or_locator", path)?, "scheme", &lp)?,
                credential_free_uri: req_str(
                    req(j, "path_or_locator", path)?,
                    "credential_free_uri",
                    &lp,
                )?,
                selector: opt_str(req(j, "path_or_locator", path)?, "selector"),
                resolved: opt_str(req(j, "path_or_locator", path)?, "resolved"),
                fetched_at: match req(j, "path_or_locator", path)?.get("fetched_at") {
                    Some(Json::Int(i)) if *i >= 0 => Some(*i as u64),
                    Some(Json::Null) | None => None,
                    _ => return Err(bad(&lp, "`fetched_at` is not a u64")),
                },
            };
            loc.validate_credential_free(&lp).map_err(|e| match e {
                RegistryError::SchemaViolation { path, detail } => {
                    ManifestError::SchemaViolation { path, detail }
                }
                other => ManifestError::SchemaViolation {
                    path: lp.clone(),
                    detail: format!("{other}"),
                },
            })?;
            if !loc.is_pinned() {
                return Err(ManifestError::UnpinnedInSealedForm {
                    detail: format!(
                        "{lp}: selector survived or resolved/fetched_at missing — \
                         a locator reaching resolved form is a pin, not a claim"
                    ),
                });
            }
            PathOrLocator::PinnedLocator(loc)
        }
        _ => {
            return Err(bad(
                path,
                "`path_or_locator` is a package path string or a pinned locator object",
            ));
        }
    };

    // declaration — an opaque TypedValue payload; never an inline record body
    // (AC-1: an inline body is a refusal — a body would carry members the
    // record schema owns; a declaration is a *reference shape*: object with
    // no `content`/`body` member carrying code).
    let dp = format!("{path}.declaration");
    let declaration = req(j, "declaration", path)?.clone();
    match &declaration {
        Json::Obj(dm) => {
            for k in dm.keys() {
                if matches!(k.as_str(), "body" | "content" | "closure" | "code") {
                    return Err(bad(
                        &dp,
                        format!(
                            "member `{k}` is an inline body — declarations name records, \
                                 they never carry them"
                        ),
                    ));
                }
            }
        }
        _ => return Err(bad(&dp, "expected object")),
    }

    let executable = match j.get("executable") {
        None | Some(Json::Null) => None,
        Some(ej) => {
            let ep = format!("{path}.executable");
            let Json::Obj(em) = ej else {
                return Err(bad(&ep, "expected object"));
            };
            for k in em.keys() {
                if !matches!(
                    k.as_str(),
                    "code_pointer"
                        | "isolation"
                        | "placement_preference"
                        | "one_shot"
                        | "host_requirements"
                        | "ext"
                ) {
                    return Err(bad(&ep, format!("unknown member `{k}`")));
                }
            }
            // code_pointer is a ContentAddress after resolve — the pin.
            let cp = format!("{ep}.code_pointer");
            let code_pointer = match ej.get("code_pointer") {
                Some(Json::Obj(cm)) => {
                    for k in cm.keys() {
                        if !matches!(
                            k.as_str(),
                            "algorithm" | "digest" | "idp" | "media_type" | "size" | "ext"
                        ) {
                            return Err(bad(&cp, format!("unknown member `{k}`")));
                        }
                    }
                    let algorithm = req_str(ej.get("code_pointer").unwrap(), "algorithm", &cp)
                        .map_err(|_| ManifestError::UnpinnedInSealedForm {
                            detail: format!("{cp}: missing `algorithm`"),
                        })?;
                    if algorithm != "sha256" {
                        return Err(ManifestError::UnpinnedInSealedForm {
                            detail: format!("{cp}: algorithm `{algorithm}` is not `sha256`"),
                        });
                    }
                    let digest = cm
                        .get("digest")
                        .and_then(Json::as_str)
                        .filter(|d| !d.is_empty())
                        .ok_or_else(|| ManifestError::UnpinnedInSealedForm {
                            detail: format!("{cp}: missing/empty `digest` — not a pin"),
                        })?;
                    let size = match cm.get("size") {
                        Some(Json::Int(i)) if *i >= 0 => *i as u64,
                        _ => {
                            return Err(ManifestError::UnpinnedInSealedForm {
                                detail: format!("{cp}: missing `size` — not a pin"),
                            })
                        }
                    };
                    ContentAddress {
                        idp: "idp/1",
                        algorithm: "sha256",
                        digest: digest.to_string(),
                        media_type: opt_str(ej.get("code_pointer").unwrap(), "media_type")
                            .unwrap_or_default(),
                        size,
                    }
                }
                _ => {
                    return Err(ManifestError::UnpinnedInSealedForm {
                        detail: format!(
                            "{cp} is not a pinned ContentAddress object — a selector or bare \
                             path is a claim, never a pin"
                        ),
                    })
                }
            };
            let isolation = IsolationClass::parse(&req_str(ej, "isolation", &ep)?)
                .ok_or_else(|| bad(&ep, "`isolation` is outside the closed IsolationClass sum"))?;
            let placement_preference = match opt_str(ej, "placement_preference") {
                Some(p) => Some(Placement::parse(&p).ok_or_else(|| {
                    bad(
                        &ep,
                        format!("`placement_preference` `{p}` is outside the closed sum"),
                    )
                })?),
                None => None,
            };
            let one_shot = match ej.get("one_shot") {
                Some(Json::Bool(b)) => Some(*b),
                None => None,
                _ => return Err(bad(&ep, "`one_shot` is not a bool")),
            };
            let host_requirements = ej
                .get("host_requirements")
                .cloned()
                .unwrap_or(Json::obj([]));
            Some(Executable {
                code_pointer,
                isolation,
                placement_preference,
                one_shot,
                host_requirements,
            })
        }
    };

    let contract_range = opt_str(j, "contract_range");

    Ok(Contribution {
        kind,
        path_or_locator,
        declaration,
        executable,
        contract_range,
    })
}

/// Whether an `ExtensionKind` is a plugin (the `register`-time dispatch test).
pub fn is_plugin(kind: &ExtensionKind) -> bool {
    matches!(kind, ExtensionKind::Plugin)
}
