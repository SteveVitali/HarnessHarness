//! The R-2.5.3⁰ exposure slice (§5d.3; ADR-0093 D9/CF-477 — the C0/Stage-1
//! rows): the catalog, the exposure plan, the revealed set, `build_catalog`,
//! `select_surfaces` with the kernel `direct_all`, `check_callable` +
//! `SurfaceNotRevealed`, the C0 `catalog_index` forms (`exact_name`,
//! `static_allowlist`) and the kernel-enforced invariants I-CLOSED … I-NOAUTH.
//!
//! Exposure **never grants or widens authority** (I-NOAUTH): `callable ⇔
//! revealed` is checked before the monitor, which runs unchanged afterwards.
//! The run-time mode sum itself is [`hh_hir::tools::ExposureMode`] (one
//! spelling source — CC7).

use std::collections::{BTreeMap, BTreeSet};

use hh_hir::tools::ExposureMode;
use hh_wire::json::Json;

use crate::equiv::SurfaceBinding;
use crate::seal::CompiledBundle;

// ── Catalog records (§5d.3 §3; ADR-0093 D2; WS-E3 §6.2) ─────────────────────

/// `CatalogEntry.source` — `{harness(component_ref) | mcp(server_ref) |
/// procedure(ref) | extension(ref) | kernel}`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CatalogSource {
    /// A harness component (`component_ref` when known).
    Harness(Option<String>),
    /// An MCP server listing.
    Mcp(String),
    /// A procedure-derived capability.
    Procedure(String),
    /// An extension contribution.
    Extension(String),
    /// A kernel-internal capability (e.g. `discover_surfaces`).
    Kernel,
}

impl CatalogSource {
    /// The canonical spelling (`<kind>` or `<kind>:<ref>`).
    pub fn as_str(&self) -> String {
        match self {
            CatalogSource::Harness(r) => format!("harness:{}", r.clone().unwrap_or_default()),
            CatalogSource::Mcp(r) => format!("mcp:{r}"),
            CatalogSource::Procedure(r) => format!("procedure:{r}"),
            CatalogSource::Extension(r) => format!("extension:{r}"),
            CatalogSource::Kernel => "kernel".to_string(),
        }
    }

    /// Parse the canonical spelling (`harness:<ref?>` tolerates the bare
    /// `harness` — `Harness(None)`).
    pub fn parse(s: &str) -> Option<CatalogSource> {
        if s == "kernel" {
            return Some(CatalogSource::Kernel);
        }
        let (kind, rest) = s.split_once(':')?;
        match kind {
            "harness" => Some(CatalogSource::Harness(if rest.is_empty() {
                None
            } else {
                Some(rest.to_string())
            })),
            "mcp" => Some(CatalogSource::Mcp(rest.to_string())),
            "procedure" => Some(CatalogSource::Procedure(rest.to_string())),
            "extension" => Some(CatalogSource::Extension(rest.to_string())),
            _ => None,
        }
    }
}

/// `IndexGranularity ∈ {surface, namespace, source, catalog}` — the
/// granularity a `catalog_index` answers at (`surface` is the C0
/// granularity; the others need a `direct` discovery capability —
/// I-DISCOVERY — and a query routed at the same granularity, R2.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IndexGranularity {
    /// One index form per surface.
    Surface,
    /// One index form per namespace.
    Namespace,
    /// One index form per source.
    Source,
    /// One index form over the catalog as a unit (§5d.2.8's `catalog`
    /// granularity — the C1 member).
    Catalog,
}

impl IndexGranularity {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            IndexGranularity::Surface => "surface",
            IndexGranularity::Namespace => "namespace",
            IndexGranularity::Source => "source",
            IndexGranularity::Catalog => "catalog",
        }
    }

    /// Parse the closed sum; unknown spellings are refused.
    pub fn parse(s: &str) -> Option<IndexGranularity> {
        match s {
            "surface" => Some(IndexGranularity::Surface),
            "namespace" => Some(IndexGranularity::Namespace),
            "source" => Some(IndexGranularity::Source),
            "catalog" => Some(IndexGranularity::Catalog),
            _ => None,
        }
    }
}

/// `search_text_fields ⊆ {name, name_split, description, param_names,
/// param_descriptions, namespace_name, namespace_description, examples,
/// effect_summary}` (ADR-0094 D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SearchTextField {
    /// `name`.
    Name,
    /// `name_split`.
    NameSplit,
    /// `description`.
    Description,
    /// `param_names`.
    ParamNames,
    /// `param_descriptions`.
    ParamDescriptions,
    /// `namespace_name`.
    NamespaceName,
    /// `namespace_description`.
    NamespaceDescription,
    /// `examples`.
    Examples,
    /// `effect_summary`.
    EffectSummary,
}

impl SearchTextField {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SearchTextField::Name => "name",
            SearchTextField::NameSplit => "name_split",
            SearchTextField::Description => "description",
            SearchTextField::ParamNames => "param_names",
            SearchTextField::ParamDescriptions => "param_descriptions",
            SearchTextField::NamespaceName => "namespace_name",
            SearchTextField::NamespaceDescription => "namespace_description",
            SearchTextField::Examples => "examples",
            SearchTextField::EffectSummary => "effect_summary",
        }
    }

    /// Parse the closed sum.
    pub fn parse(s: &str) -> Option<SearchTextField> {
        match s {
            "name" => Some(SearchTextField::Name),
            "name_split" => Some(SearchTextField::NameSplit),
            "description" => Some(SearchTextField::Description),
            "param_names" => Some(SearchTextField::ParamNames),
            "param_descriptions" => Some(SearchTextField::ParamDescriptions),
            "namespace_name" => Some(SearchTextField::NamespaceName),
            "namespace_description" => Some(SearchTextField::NamespaceDescription),
            "examples" => Some(SearchTextField::Examples),
            "effect_summary" => Some(SearchTextField::EffectSummary),
            _ => None,
        }
    }
}

/// `IndexForm{name, title?, summary: Ref<Text>, namespace?, tags[],
/// search_text_fields}` — the typed handle an `indexed` entry delivers
/// (`handle_only`; ADR-0093 D1). `summary` is a `Ref<Text>` — the leaf's
/// provenance label joins `context_label` when the form is delivered
/// (I-LABEL).
#[derive(Debug, Clone, PartialEq)]
pub struct IndexForm {
    /// The model-facing name.
    pub name: String,
    /// The display title.
    pub title: Option<String>,
    /// The summary `Text` leaf ref (a `content_hash` at C0).
    pub summary_ref: String,
    /// The namespace.
    pub namespace: Option<String>,
    /// Tags.
    pub tags: Vec<String>,
    /// The fields an index may read.
    pub search_text_fields: BTreeSet<SearchTextField>,
    /// The materialized text of declared search fields whose content lives
    /// behind a `Ref` (S2.10 — C1 indexes read text, not refs):
    /// `description`, `param_names`, `param_descriptions`,
    /// `namespace_description`, `examples` populate here when the catalog
    /// builder/lowering supplies them. Absent ⇒ the field contributes no
    /// text (a declared-but-absent field is honest `n/a`, never invented).
    /// An index only reads a member when the field is also declared in
    /// `search_text_fields` (I-NARROW — the declaration gates readability).
    pub search_text: BTreeMap<SearchTextField, String>,
}

/// `permission_coverage ∈ {covered, ask, uncovered, unknown}` — a selection
/// input only (I-NOAUTH: authority is decided by H1 at call time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionCoverage {
    /// A grant covers the declared effects.
    Covered,
    /// A grant can be raised (`permission_request` is admissible).
    Ask,
    /// No grant covers the effects.
    Uncovered,
    /// Coverage is not determined.
    Unknown,
}

/// `availability ∈ {available, unavailable(reason), stale(epoch)}`.
#[derive(Debug, Clone, PartialEq)]
pub enum Availability {
    /// Callable now.
    Available,
    /// Not callable — `reason` ∈ {`removed`, `untrusted`, `unattested`, …}.
    Unavailable(String),
    /// From a prior catalog epoch (I-EPOCH).
    Stale(u64),
}

/// `CatalogEntry{surface_id, capability: semantic_id, version_id, source,
/// namespace?, admitted_modes, pinned, index_form, size{tokens,
/// estimator_ref}, effect_summary, permission_coverage, label, availability}`.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogEntry {
    /// The `SurfaceBinding.surface_id`.
    pub surface_id: String,
    /// The capability's `semantic_id`.
    pub capability: String,
    /// The capability's pinned `version_id`.
    pub version_id: String,
    /// The catalog source.
    pub source: CatalogSource,
    /// The model-facing name (`surface_name` — what a proposal names).
    pub name: String,
    /// The namespace.
    pub namespace: Option<String>,
    /// `definition_modes` — narrowed by profile/policy at `select_surfaces`
    /// (I-NARROW).
    pub admitted_modes: BTreeSet<ExposureMode>,
    /// Definition/kernel-set; immutable to policy. Pinned ⇒ `direct`.
    pub pinned: bool,
    /// `hidden` — never delivered, indexed or callable.
    pub hidden: bool,
    /// The typed index form.
    pub index_form: IndexForm,
    /// `size.tokens` — the direct-delivery estimate.
    pub size_tokens: u64,
    /// `size.estimator_ref` — the estimator that produced `size_tokens`.
    pub estimator_ref: String,
    /// The ADR-0021 hint projection of declared effect domains (never
    /// authority).
    pub effect_summary: Vec<String>,
    /// The derived permission coverage.
    pub permission_coverage: PermissionCoverage,
    /// The entry's label (joins `context_label` — I-LABEL).
    pub label: Option<String>,
    /// Availability.
    pub availability: Availability,
    /// Whether the entry is the `discover_surfaces` capability
    /// (`exposure_hint.discovery = true` — I-DISCOVERY reads it).
    pub is_discovery: bool,
    /// `lifted` — the entry joined the catalog through an unverified lift
    /// (an MCP/extension import, an `adopt`ed addition — the delta's
    /// `authority = unverified` stamp's entry-side half; R2.8). The
    /// attestation-gated indexing leg (`SelectionGates.index_unverified`)
    /// refuses `indexed` on lifted entries; a `lifted` ∧ `pinned` entry is
    /// a `CatalogBuildError::PinBindsLifted`/`CatalogDriftError` —
    /// a pin binds the admission, never an unverified lift.
    pub lifted: bool,
}

/// `sources[{source_ref, snapshot_hash, ttl?, listened}]` — one row per
/// catalog source.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceState {
    /// The source ref.
    pub source_ref: String,
    /// The source snapshot hash.
    pub snapshot_hash: String,
    /// The freshness hint (`ttlMs`).
    pub ttl: Option<u64>,
    /// Whether `list_changed` notifications are listened to.
    pub listened: bool,
}

/// `Catalog{catalog_id = H(canonical(entries by surface_id)), epoch,
/// bundle_id, sources[], entries[], index_ref}` — content-addressed per epoch
/// (I-EPOCH: `catalog_id` changes only through `adopt`).
#[derive(Debug, Clone, PartialEq)]
pub struct Catalog {
    /// `H(canonical entries by surface_id)`.
    pub catalog_id: String,
    /// The catalog epoch.
    pub epoch: u64,
    /// The bundle the entries were compiled from.
    pub bundle_id: String,
    /// The source states.
    pub sources: Vec<SourceState>,
    /// The entries (sorted by `surface_id` — canonical).
    pub entries: Vec<CatalogEntry>,
    /// The bound `catalog_index` declarations — `(index, granularity)`
    /// pairs (§5d.2.8; R2.8). The per-surface indexes discovery serves are
    /// here; non-surface granularities (`namespace`/`source`/`catalog`)
    /// need a `direct` discovery capability (I-DISCOVERY) and answer at
    /// their declared granularity.
    pub indexes: Vec<IndexDecl>,
    /// The content-addressed index ref — `H(canonical indexes[])` when any
    /// are bound ([`bind_indexes`]); `None` when unbound.
    pub index_ref: Option<String>,
}

/// `IndexDecl{index, granularity}` — one bound `catalog_index` declaration
/// (§5d.2 §3; the granularity the index answers at — the `index_forms_at`
/// producer's read).
#[derive(Debug, Clone, PartialEq)]
pub struct IndexDecl {
    /// The index variant.
    pub index: CatalogIndex,
    /// The granularity the index answers at.
    pub granularity: IndexGranularity,
}

/// `index_decl_json` — the canonical declaration record.
pub fn index_decl_json(d: &IndexDecl) -> Json {
    Json::obj([
        ("granularity", Json::str(d.granularity.as_str())),
        ("index", index_json(&d.index)),
    ])
}

/// `bind_indexes(catalog, decls)` — declare the catalog's index set;
/// `index_ref` re-mints over the canonical declarations (`index_ref` is the
/// content address of the *declared set*, not of any built posting list —
/// the build's instrument stamp, ADR-0094 D4).
pub fn bind_indexes(catalog: &mut Catalog, decls: Vec<IndexDecl>) {
    let body = Json::Arr(decls.iter().map(index_decl_json).collect());
    catalog.index_ref = (!decls.is_empty())
        .then(|| hh_identity::idp::idp_id("catalog_index", body.to_canonical_string().as_bytes()));
    catalog.indexes = decls;
}

// ── The plan / revealed set (ADR-0093 D4/D6) ─────────────────────────────────

/// `omitted.reason` — the closed omission-reason sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OmitReason {
    /// Over the direct-token share.
    Budget,
    /// The policy withheld it.
    Policy,
    /// `permission_gate = hide_uncovered` removed it (C2).
    PermissionUncovered,
    /// `availability ≠ available`.
    Unavailable,
    /// `hidden` (AC-R-2.5.3-12 — the omission *record* is how a hidden entry
    /// is accounted for; it never renders).
    Hidden,
    /// No admitted mode survived the profile/policy meet.
    ModeUnsupportedByProfile,
}

impl OmitReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            OmitReason::Budget => "budget",
            OmitReason::Policy => "policy",
            OmitReason::PermissionUncovered => "permission_uncovered",
            OmitReason::Unavailable => "unavailable",
            OmitReason::Hidden => "hidden",
            OmitReason::ModeUnsupportedByProfile => "mode_unsupported_by_profile",
        }
    }
}

/// An `omitted[]` member.
#[derive(Debug, Clone, PartialEq)]
pub struct OmittedEntry {
    /// The surface.
    pub surface_id: String,
    /// Why it was omitted.
    pub reason: OmitReason,
}

/// `budget_estimate{tokens, estimator_ref}`.
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetEstimate {
    /// Estimated direct-delivery tokens.
    pub tokens: u64,
    /// The estimator.
    pub estimator_ref: String,
}

/// `ExposurePlan{plan_id, model_call_id, catalog_id, entries[(surface_id,
/// mode)], discovery_surface_ids[], order, budget_estimate, omitted[],
/// derived_from, deterministic}` — ledger data (`action.tool.exposure.planned`
/// carries the mode changes plus `catalog_id`; the full vector is a
/// projection).
#[derive(Debug, Clone, PartialEq)]
pub struct ExposurePlan {
    /// `H(canonical plan sans plan_id)`.
    pub plan_id: String,
    /// The producing model call.
    pub model_call_id: String,
    /// The catalog the plan was built over.
    pub catalog_id: String,
    /// `entries[(surface_id, mode)]` — I-ONE-MODE: exactly one mode per
    /// surface.
    pub entries: Vec<(String, ExposureMode)>,
    /// The `discover_surfaces` surfaces `direct` in this plan.
    pub discovery_surface_ids: Vec<String>,
    /// The delivery order — I-ORDER: a prefix-extension of the previous plan.
    pub order: Vec<String>,
    /// The direct-token estimate.
    pub budget_estimate: BudgetEstimate,
    /// The omitted entries with reasons.
    pub omitted: Vec<OmittedEntry>,
    /// What the plan derives from (`catalog_id ∥ turn_state` digest).
    pub derived_from: String,
    /// `true` — `select_surfaces` is deterministic over its inputs.
    pub deterministic: bool,
}

/// `cause ∈ {policy, discovery, principal, procedure, pinned}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevealCause {
    /// The exposure policy revealed it.
    Policy,
    /// A `discover_surfaces` result revealed it.
    Discovery,
    /// The principal named it.
    Principal,
    /// A `procedure_index` expansion revealed it.
    Procedure,
    /// Revealed by pinning.
    Pinned,
}

impl RevealCause {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RevealCause::Policy => "policy",
            RevealCause::Discovery => "discovery",
            RevealCause::Principal => "principal",
            RevealCause::Procedure => "procedure",
            RevealCause::Pinned => "pinned",
        }
    }

    /// Parse the closed sum.
    pub fn parse(s: &str) -> Option<RevealCause> {
        match s {
            "policy" => Some(RevealCause::Policy),
            "discovery" => Some(RevealCause::Discovery),
            "principal" => Some(RevealCause::Principal),
            "procedure" => Some(RevealCause::Procedure),
            "pinned" => Some(RevealCause::Pinned),
            _ => None,
        }
    }
}

/// `retention ∈ {call, turn, run, until_evicted}` — default `run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retention {
    /// The producing call only.
    Call,
    /// The producing turn.
    Turn,
    /// The run (the default).
    Run,
    /// Until explicitly evicted.
    UntilEvicted,
}

impl Retention {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Retention::Call => "call",
            Retention::Turn => "turn",
            Retention::Run => "run",
            Retention::UntilEvicted => "until_evicted",
        }
    }

    /// Parse the closed sum.
    pub fn parse(s: &str) -> Option<Retention> {
        match s {
            "call" => Some(Retention::Call),
            "turn" => Some(Retention::Turn),
            "run" => Some(Retention::Run),
            "until_evicted" => Some(Retention::UntilEvicted),
            _ => None,
        }
    }
}

/// A `RevealedSet` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct RevealedEntry {
    /// The surface.
    pub surface_id: String,
    /// The mode it was revealed under.
    pub mode: ExposureMode,
    /// The reveal seq.
    pub revealed_at: u64,
    /// The cause.
    pub cause: RevealCause,
    /// The retention.
    pub retention: Retention,
}

/// `RevealedSet{run_id, entries[]}` — run state projected from
/// `action.tool.surface.revealed/evicted` (never authored).
#[derive(Debug, Clone, PartialEq)]
pub struct RevealedSet {
    /// The run.
    pub run_id: String,
    /// The revealed entries.
    pub entries: Vec<RevealedEntry>,
}

/// `evict.cause ∈ {compaction, policy, principal}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictCause {
    /// The compactor evicted it (§05c D2).
    Compaction,
    /// The policy evicted it.
    Policy,
    /// The principal evicted it.
    Principal,
}

impl EvictCause {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            EvictCause::Compaction => "compaction",
            EvictCause::Policy => "policy",
            EvictCause::Principal => "principal",
        }
    }

    /// Parse the closed sum.
    pub fn parse(s: &str) -> Option<EvictCause> {
        match s {
            "compaction" => Some(EvictCause::Compaction),
            "policy" => Some(EvictCause::Policy),
            "principal" => Some(EvictCause::Principal),
            _ => None,
        }
    }
}

/// `drift_policy ∈ {freeze, adopt, ask}` — `freeze` for Lab runs, `adopt` for
/// interactive (ADR-0095 D2); `ask` is a `permission_request` (C2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftPolicy {
    /// Refuse drift: removed ⇒ `unavailable(removed)`, `catalog_id` unchanged.
    Freeze,
    /// Adopt deltas as incremental lowerings in a new epoch.
    Adopt,
    /// Raise a `permission_request` (C2).
    Ask,
}

/// `permission_gate ∈ {none, hide_uncovered, demote_uncovered}` — OQ-234's
/// ratified default is `demote_uncovered` at C0/C1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionGate {
    /// Coverage is not a selection input.
    None,
    /// `uncovered` surfaces are hidden from the model (C2).
    HideUncovered,
    /// `uncovered` surfaces may be demoted by policy (the C0/C1 default).
    DemoteUncovered,
}

/// `discovery_executor_preference ∈ {kernel, provider, provider_if_declared}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryExecutorPreference {
    /// The kernel index answers.
    Kernel,
    /// A provider-hosted search answers.
    Provider,
    /// Provider when declared, kernel otherwise.
    ProviderIfDeclared,
}

/// `ExposurePolicyParams` — every parameter MUST-data (ADR-0093 D3).
#[derive(Debug, Clone, PartialEq)]
pub struct ExposurePolicyParams {
    /// `source kind → default mode` (e.g. `{"mcp": "deferred"}`).
    pub default_mode_by_source: BTreeMap<String, ExposureMode>,
    /// `pinned_direct[]` — policy-declared pins (narrow-only: they can only
    /// confirm what the definition already admitted `direct`).
    pub pinned_direct: Vec<String>,
    /// A bound on direct entries.
    pub max_direct_count: Option<u64>,
    /// `direct_token_share` — the share of `window_cap` direct surfaces may
    /// occupy (I-BUDGET).
    pub direct_token_share: f64,
    /// The index granularity.
    pub index_granularity: IndexGranularity,
    /// `max_reveal_per_search` (default 8; ceiling 32 — ADR-0094 D4).
    pub max_reveal_per_search: u64,
    /// The default reveal retention.
    pub retention: Retention,
    /// The discovery executor preference.
    pub discovery_executor_preference: DiscoveryExecutorPreference,
    /// The drift policy.
    pub drift_policy: DriftPolicy,
    /// The permission gate (default `demote_uncovered` — OQ-234).
    pub permission_gate: PermissionGate,
}

impl Default for ExposurePolicyParams {
    fn default() -> Self {
        ExposurePolicyParams {
            default_mode_by_source: BTreeMap::new(),
            pinned_direct: Vec::new(),
            max_direct_count: None,
            direct_token_share: 1.0,
            index_granularity: IndexGranularity::Surface,
            max_reveal_per_search: 8,
            retention: Retention::Run,
            discovery_executor_preference: DiscoveryExecutorPreference::Kernel,
            drift_policy: DriftPolicy::Adopt,
            permission_gate: PermissionGate::DemoteUncovered,
        }
    }
}

/// `turn_state{model_call_id, revealed, recent_calls[], goal_ref?, budget
/// share + counters, prior_order}` — the per-call selection input.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnState {
    /// The producing model call.
    pub model_call_id: String,
    /// The run's revealed set.
    pub revealed: RevealedSet,
    /// Recent tool-call surface ids.
    pub recent_calls: Vec<String>,
    /// The current goal ref.
    pub goal_ref: Option<String>,
    /// The call's context `window_cap` in tokens (I-BUDGET's bound).
    pub window_cap: u64,
    /// The previous plan's `order` (I-ORDER: this plan's order must be a
    /// prefix-extension of it).
    pub prior_order: Vec<String>,
}

// ── Discovery (ADR-0094 D1/D4) ───────────────────────────────────────────────

/// `DiscoveryForm ∈ {regex, natural_language, structured{...}}`.
#[derive(Debug, Clone, PartialEq)]
pub enum DiscoveryForm {
    /// A regex/verbatim pattern.
    Regex,
    /// Free text.
    NaturalLanguage,
    /// The structured form.
    Structured(StructuredQuery),
}

/// `structured{namespace?, effect_filter?, tags?, source?}`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StructuredQuery {
    /// The namespace filter.
    pub namespace: Option<String>,
    /// An effect-domain filter.
    pub effect_filter: Option<String>,
    /// Tag filters.
    pub tags: Vec<String>,
    /// A source filter.
    pub source: Option<String>,
}

/// `DiscoveryQuery{form, text, limit?, granularity}` — the
/// `discover_surfaces` input. `granularity` is the R2.8 member (default
/// `surface`): a non-`surface` query is served by an [`IndexDecl`] whose
/// declared granularity matches — hits are the member entries of the
/// matched units (§5d.2.8's non-surface granularity leg).
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveryQuery {
    /// The form.
    pub form: DiscoveryForm,
    /// The query text (`""` is `DiscoveryQueryInvalid::Empty`).
    pub text: String,
    /// The caller's hit bound.
    pub limit: Option<u64>,
    /// The granularity the query answers at (default `surface`).
    pub granularity: IndexGranularity,
}

impl Default for DiscoveryQuery {
    fn default() -> Self {
        DiscoveryQuery {
            form: DiscoveryForm::NaturalLanguage,
            text: String::new(),
            limit: None,
            granularity: IndexGranularity::Surface,
        }
    }
}

/// `DiscoveryQueryInvalid{too_long | empty | unsupported_form}` —
/// `invalid_pattern` is the S2.10 addition (a closed-sum extension — the
/// `lexical_regex` variant refuses malformed patterns rather than coercing
/// them; recorded in the S2.10 ADR).
#[derive(Debug, Clone, PartialEq)]
pub enum DiscoveryQueryInvalid {
    /// The text exceeded the bound (4096 chars).
    TooLong,
    /// The text was empty.
    Empty,
    /// The index variant cannot serve the form (C1+ indexes declare forms).
    UnsupportedForm,
    /// The `regex` text failed the bounded engine's syntax/limits
    /// (`lexical_regex` only; reason from [`crate::regex::RegexError`]).
    InvalidPattern {
        /// The closed reason tag.
        reason: String,
    },
    /// The index variant needs a declared [`IndexExecutor`] (the C2
    /// `embedding`/`model_ranked`/`filesystem` variants carry no kernel
    /// engine) and none is bound — the typed refusal, never a fake ranking.
    ExecutorUnavailable,
    /// The index is declared at a different granularity than the query
    /// (§5d.2.8 — an index answers only at its declared granularity).
    NotQueryable {
        /// The granularity the index serves.
        index_granularity: IndexGranularity,
        /// The granularity the query asked.
        query_granularity: IndexGranularity,
    },
}

/// A `DiscoveryResult.hits[]` member — `{surface_id, rank, score?}`.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveryHit {
    /// The surface.
    pub surface_id: String,
    /// Its rank.
    pub rank: u64,
    /// The score, when the index produces one (provider search: `n/a`).
    pub score: Option<Json>,
}

/// `DiscoveryResult{hits[], truncated, executed_by, cost}` — index forms
/// only, never definitions.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveryResult {
    /// The hits (ordered by rank).
    pub hits: Vec<DiscoveryHit>,
    /// Whether the hit set was truncated by the reveal bound.
    pub truncated: bool,
    /// `kernel` | `provider`.
    pub executed_by: String,
    /// The cost record, when measured.
    pub cost: Option<Json>,
}

/// The `catalog_index` variants (ADR-0094 D4): **C0** `exact_name` and
/// `static_allowlist`; **C1** (S2.10) `lexical_regex`, `bm25`,
/// `hierarchical`. `hidden` entries are never indexed (I-CLOSED ∩
/// AC-R-2.5.3-12).
///
/// Form admissibility (each C1 variant *declares* the forms it serves —
/// anything else is `DiscoveryQueryInvalid::UnsupportedForm`):
///
/// | variant | `regex` | `natural_language` | `structured` |
/// |---|---|---|---|
/// | `exact_name` | verbatim `name` | verbatim `name` | filters + verbatim `name` |
/// | `static_allowlist` | verbatim `name` | verbatim `name` | filters only |
/// | `lexical_regex` | bounded regex over declared search text | refused | filters + regex over text |
/// | `bm25` | refused | BM25 over declared search text | filters + BM25 |
/// | `hierarchical` | refused | namespace-path descent | filters + descent |
/// | `embedding{t}` | refused | executor-scored (cosine ≥ t) | filters + executor |
/// | `model_ranked` | refused | executor-ranked | filters + executor |
/// | `filesystem` | path glob (executor) | refused | filters + executor |
///
/// **C2 executor variants** (R2.8): `embedding`, `model_ranked` and
/// `filesystem` declare their forms but carry no kernel embedding/model/
/// filesystem engine — they execute through a declared
/// [`IndexExecutor`]; an absent executor is
/// [`DiscoveryQueryInvalid::ExecutorUnavailable`] (honest refusal, never a
/// fake ranking — T-LCD-15). `executed_by` on their result names the
/// executor leg (`provider`/`filesystem`), never `"kernel"`.
#[derive(Debug, Clone, PartialEq)]
pub enum CatalogIndex {
    /// `exact_name` — the query text matches `IndexForm.name` literally.
    ExactName,
    /// `static_allowlist` — the declared surface-id set, filtered by the
    /// query's structured members.
    StaticAllowlist(BTreeSet<String>),
    /// `lexical_regex` — the query text compiles to the bounded
    /// [`crate::regex::Regex`] NFA and matches against the entry's declared
    /// search text (any field matching is a hit; `hit.score = n/a` — regex
    /// is a predicate, not a ranker, so hits order by `surface_id`).
    LexicalRegex,
    /// `bm25` — BM25 over the tokenized declared search text with field
    /// weights (`name` 4.0, `name_split`/`namespace_name` 2.0,
    /// `description` 1.5, others 1.0); `k1 = 1.2`, `b = 0.75`; `score` is the
    /// BM25 value scaled ×10⁶ as an integer (canonical JSON carries no
    /// float — the scale is declared here, not per-hit).
    Bm25,
    /// `hierarchical` — namespace-path descent: query segments (split on
    /// `.`, `::`, `/`) descend the entry's `namespace ∥ name` path; score =
    /// matched segment depth (×10⁶ int) with a terminal name-prefix bonus.
    Hierarchical,
    /// `embedding{similarity_threshold}` — an embedding-similarity index:
    /// hits are entries whose executor-computed similarity meets the
    /// declared threshold (×10⁶-scaled — canonical JSON carries no float).
    /// C2: executor-gated.
    Embedding {
        /// The similarity threshold (×10⁶-scaled — `0..=1_000_000`).
        similarity_threshold_ppm: u64,
    },
    /// `model_ranked` — a model/provider-ranked index (C2 — executor-gated;
    /// the ranker is a declared instrument, `executed_by` names it).
    ModelRanked,
    /// `filesystem` — a filesystem catalogue index (path-glob over the
    /// declared entries — C2 — executor-gated).
    Filesystem,
}

/// The canonical JSON of an index declaration (the `index_ref` preimage and
/// the `IndexDecl` member spelling).
pub fn index_json(index: &CatalogIndex) -> Json {
    match index {
        CatalogIndex::ExactName => Json::obj([("kind", Json::str("exact_name"))]),
        CatalogIndex::StaticAllowlist(ids) => Json::obj([
            ("kind", Json::str("static_allowlist")),
            (
                "surface_ids",
                Json::Arr(ids.iter().map(|i| Json::str(i.clone())).collect()),
            ),
        ]),
        CatalogIndex::LexicalRegex => Json::obj([("kind", Json::str("lexical_regex"))]),
        CatalogIndex::Bm25 => Json::obj([("kind", Json::str("bm25"))]),
        CatalogIndex::Hierarchical => Json::obj([("kind", Json::str("hierarchical"))]),
        CatalogIndex::Embedding {
            similarity_threshold_ppm,
        } => Json::obj([
            ("kind", Json::str("embedding")),
            (
                "similarity_threshold_ppm",
                Json::Int(*similarity_threshold_ppm as i64),
            ),
        ]),
        CatalogIndex::ModelRanked => Json::obj([("kind", Json::str("model_ranked"))]),
        CatalogIndex::Filesystem => Json::obj([("kind", Json::str("filesystem"))]),
    }
}

/// The content-addressed index ref (`charged instrument` — the ref is
/// `H(canonical index form)`).
pub fn index_ref(index: &CatalogIndex) -> String {
    hh_identity::idp::idp_id(
        "catalog_index",
        index_json(index).to_canonical_string().as_bytes(),
    )
}

/// `DiscoveryFormKind ∈ {regex, natural_language, structured}` — the form
/// *kind* a `catalog_index` declares it serves (the `index_forms_at`
/// producer's vocabulary; the `DiscoveryForm` value carries the structured
/// members — the kind is the admissibility set member).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiscoveryFormKind {
    /// `regex`.
    Regex,
    /// `natural_language`.
    NaturalLanguage,
    /// `structured`.
    Structured,
}

impl DiscoveryFormKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DiscoveryFormKind::Regex => "regex",
            DiscoveryFormKind::NaturalLanguage => "natural_language",
            DiscoveryFormKind::Structured => "structured",
        }
    }

    /// The kind of a query form.
    pub fn of(form: &DiscoveryForm) -> DiscoveryFormKind {
        match form {
            DiscoveryForm::Regex => DiscoveryFormKind::Regex,
            DiscoveryForm::NaturalLanguage => DiscoveryFormKind::NaturalLanguage,
            DiscoveryForm::Structured(_) => DiscoveryFormKind::Structured,
        }
    }
}

/// The form kinds an index serves — the doc-table above as data (the
/// `catalog_index` capability schema's `forms` member and
/// `index_forms_at`'s per-granularity answer).
pub fn form_kinds(index: &CatalogIndex) -> BTreeSet<DiscoveryFormKind> {
    use DiscoveryFormKind::*;
    match index {
        // verbatim `name` serves every form.
        CatalogIndex::ExactName | CatalogIndex::StaticAllowlist(_) => {
            [Regex, NaturalLanguage, Structured].into_iter().collect()
        }
        // a regex engine answers no free-text.
        CatalogIndex::LexicalRegex => [Regex, Structured].into_iter().collect(),
        // BM25/descent/embeddings/model-rank answer no regex query.
        CatalogIndex::Bm25
        | CatalogIndex::Hierarchical
        | CatalogIndex::Embedding { .. }
        | CatalogIndex::ModelRanked => [NaturalLanguage, Structured].into_iter().collect(),
        // `filesystem` answers the glob form (a `regex`-class query over
        // paths) and the structured filters — natural-language prose has no
        // path semantics to bind to.
        CatalogIndex::Filesystem => [Regex, Structured].into_iter().collect(),
    }
}

/// The structured members of a query (empty for non-structured forms).
fn structured_of(query: &DiscoveryQuery) -> Option<&StructuredQuery> {
    match &query.form {
        DiscoveryForm::Structured(s) => Some(s),
        _ => None,
    }
}

/// The structured filters — namespace, effect, tags, source — applied to
/// every variant (a `structured` member narrows the candidate set; the
/// variant decides how `text` matches).
fn matches_structured(e: &CatalogEntry, s: &StructuredQuery) -> bool {
    if let Some(ns) = &s.namespace {
        if e.namespace.as_deref() != Some(ns.as_str()) {
            return false;
        }
    }
    if let Some(d) = &s.effect_filter {
        if !e.effect_summary.iter().any(|x| x == d) {
            return false;
        }
    }
    if !s.tags.iter().all(|t| e.index_form.tags.contains(t)) {
        return false;
    }
    if let Some(src) = &s.source {
        if !e.source.as_str().starts_with(src.as_str()) {
            return false;
        }
    }
    true
}

/// The declared, readable search text of an entry — `(field, weight, text)`
/// triples. A field contributes only when declared in `search_text_fields`
/// AND its text is available: `name`, `name_split`, `namespace_name` and
/// `effect_summary` are inline on the entry; the rest are read from
/// `index_form.search_text` (the materialized form — absent means the field
/// contributes nothing, never invented text).
fn search_fields(e: &CatalogEntry) -> Vec<(SearchTextField, f64, String)> {
    let mut out = Vec::new();
    let declared = |f: SearchTextField| e.index_form.search_text_fields.contains(&f);
    if declared(SearchTextField::Name) {
        out.push((SearchTextField::Name, 4.0, e.index_form.name.clone()));
    }
    if declared(SearchTextField::NameSplit) {
        out.push((SearchTextField::NameSplit, 2.0, name_split(&e.name)));
    }
    if declared(SearchTextField::NamespaceName) {
        if let Some(ns) = &e.namespace {
            out.push((SearchTextField::NamespaceName, 2.0, ns.clone()));
        }
    }
    if declared(SearchTextField::EffectSummary) && !e.effect_summary.is_empty() {
        out.push((
            SearchTextField::EffectSummary,
            1.0,
            e.effect_summary.join(" "),
        ));
    }
    for (field, weight) in [
        (SearchTextField::Description, 1.5),
        (SearchTextField::ParamNames, 1.0),
        (SearchTextField::ParamDescriptions, 0.75),
        (SearchTextField::NamespaceDescription, 1.0),
        (SearchTextField::Examples, 1.0),
    ] {
        if declared(field) {
            if let Some(text) = e.index_form.search_text.get(&field) {
                if !text.is_empty() {
                    out.push((field, weight, text.clone()));
                }
            }
        }
    }
    out
}

/// `name_split` — split a surface name on separators and case boundaries:
/// `fs.readFile` → `fs read file`. Deterministic, allocation-light.
fn name_split(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 8);
    let mut prev_lower_or_digit = false;
    for c in name.chars() {
        if c.is_alphanumeric() {
            if prev_lower_or_digit && c.is_uppercase() {
                out.push(' ');
            }
            out.push(c);
            prev_lower_or_digit = c.is_lowercase() || c.is_ascii_digit();
        } else {
            out.push(' ');
            prev_lower_or_digit = false;
        }
    }
    out
}

/// The tokenizer shared by `bm25` and `hierarchical` — lowercase ASCII
/// alphanumeric runs.
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect()
}

/// One scored hit — `(surface_id, score)` where `score` is the ×10⁶-scaled
/// integer the result carries (`None` for predicate indexes).
type Scored = (String, Option<i64>);

/// `bm25` ranking — field-weighted term frequencies (see `search_fields`),
/// `k1 = 1.2`, `b = 0.75`, Robertson–Zaragoza IDF `ln(1 + (N − n + .5)/(n +
/// .5))`. Scores scale ×10⁶ into `i64` for canonical JSON.
fn bm25_rank<'e>(entries: &[&'e CatalogEntry], query_text: &str) -> Vec<(&'e CatalogEntry, i64)> {
    const K1: f64 = 1.2;
    const B: f64 = 0.75;
    let q_terms: Vec<String> = {
        let mut t = tokenize(query_text);
        t.sort();
        t.dedup();
        t
    };
    if q_terms.is_empty() {
        return Vec::new();
    }
    // Per-entry weighted doc: term → weighted freq, plus weighted length.
    let docs: Vec<(&CatalogEntry, BTreeMap<String, f64>, f64)> = entries
        .iter()
        .map(|e| {
            let mut tf: BTreeMap<String, f64> = BTreeMap::new();
            let mut len = 0.0;
            for (_, w, text) in search_fields(e) {
                for tok in tokenize(&text) {
                    *tf.entry(tok).or_insert(0.0) += w;
                    len += w;
                }
            }
            (*e, tf, len)
        })
        .collect();
    let n_docs = docs.len() as f64;
    let avgdl = if n_docs > 0.0 {
        docs.iter().map(|(_, _, l)| l).sum::<f64>() / n_docs
    } else {
        1.0
    };
    let avgdl = if avgdl > 0.0 { avgdl } else { 1.0 };
    // Document frequency per query term.
    let mut df: BTreeMap<&str, usize> = BTreeMap::new();
    for t in &q_terms {
        let d = docs.iter().filter(|(_, tf, _)| tf.contains_key(t)).count();
        df.insert(t.as_str(), d);
    }
    let mut scored: Vec<(&CatalogEntry, i64)> = docs
        .iter()
        .map(|(e, tf, dl)| {
            let mut s = 0.0;
            for t in &q_terms {
                let Some(f) = tf.get(t) else { continue };
                let n = *df.get(t.as_str()).unwrap_or(&0) as f64;
                let idf = (1.0 + (n_docs - n + 0.5) / (n + 0.5)).ln();
                s += idf * (f * (K1 + 1.0)) / (f + K1 * (1.0 - B + B * dl / avgdl));
            }
            (*e, (s * 1_000_000.0).round() as i64)
        })
        .collect();
    scored.retain(|(_, s)| *s > 0);
    scored.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.surface_id.cmp(&b.0.surface_id))
    });
    scored
}

/// The path segments of a namespace/name string — split on `.`, `::`, `/`,
/// `-`, `_`; lowercased.
fn path_segments(s: &str) -> Vec<String> {
    s.split(['.', ':', '/', '-', '_', ' '])
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect()
}

/// `hierarchical` ranking — query segments descend the entry's
/// `namespace ∥ name` path. Score (pre-scale) = `2 × matched_prefix_depth`
/// + `1` when the last query segment prefix-matches the surface name; a hit
///   requires ≥1 matched segment. Segments beyond the namespace compare
///   against the name split.
fn hierarchical_rank<'e>(
    entries: &[&'e CatalogEntry],
    query_text: &str,
) -> Vec<(&'e CatalogEntry, i64)> {
    let q = path_segments(query_text);
    if q.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(&CatalogEntry, i64)> = entries
        .iter()
        .filter_map(|e| {
            let mut path: Vec<String> = e
                .namespace
                .as_deref()
                .map(path_segments)
                .unwrap_or_default();
            path.extend(path_segments(&e.name));
            // Longest common prefix length of query segments vs path.
            let mut depth = 0usize;
            for (qs, ps) in q.iter().zip(path.iter()) {
                if qs == ps {
                    depth += 1;
                } else {
                    break;
                }
            }
            // A single-segment query also descends by name alone (the
            // segment matches any path segment or name-prefix).
            let name_hit = q.last().is_some_and(|last| {
                path_segments(&e.name)
                    .iter()
                    .any(|n| n.starts_with(last.as_str()))
            });
            if depth == 0 && !name_hit {
                return None;
            }
            let mut score = (2 * depth) as i64;
            if name_hit {
                score += 1;
            }
            Some((*e, score * 1_000_000))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.surface_id.cmp(&b.0.surface_id))
    });
    scored
}

/// `IndexExecutor` — the declared executor legs for the C2 index variants
/// (R2.8). Every member is a function pointer the host binds (the driver,
/// a test fixture, an MCP-side search); the kernel ships no embedding /
/// model / filesystem engine of its own — an absent leg is
/// [`DiscoveryQueryInvalid::ExecutorUnavailable`], never a fabricated
/// ranking (T-LCD-15).
///
/// Executor contract: `(candidates, query) → (surface_id, score?)` hits in
/// the executor's rank order; `Err(detail)` is the executor's typed
/// failure (the query fails as `ExecutorUnavailable` with the detail on
/// the refusal — detail carried for the audit row, never the result).
/// The executor-leg callback — a variant's host-side index answer:
/// `[(surface_id, score?)]` over the caller-filtered candidates; a `None`
/// score is a rank-only hit (provider search carries no score), `Err` is
/// the executor's typed failure (the query fails `ExecutorUnavailable`).
pub type ExecutorFn =
    fn(&[&CatalogEntry], &DiscoveryQuery) -> Result<Vec<(String, Option<i64>)>, String>;

#[derive(Clone, Copy, Default)]
pub struct IndexExecutor {
    /// The `embedding` leg — executor-scored cosine over the declared
    /// search text; hits below the variant's `similarity_threshold_ppm`
    /// are dropped by the kernel after the executor answers.
    pub embedding: Option<ExecutorFn>,
    /// The `model_ranked` leg.
    pub model_ranked: Option<ExecutorFn>,
    /// The `filesystem` leg — the host's path-glob walker.
    pub filesystem: Option<ExecutorFn>,
}

impl std::fmt::Debug for IndexExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IndexExecutor")
            .field("embedding", &self.embedding.is_some())
            .field("model_ranked", &self.model_ranked.is_some())
            .field("filesystem", &self.filesystem.is_some())
            .finish()
    }
}

/// The per-variant matcher — returns scored hits over `candidates` (the
/// caller's filtered set: non-hidden, plan-mode-restricted, structured
/// filters already applied is the caller's choice — this layer applies
/// `matches_structured` itself so every variant honours it uniformly).
/// `executed_by` names the executor leg the row records (`kernel` for the
/// in-tree variants; `provider`/`filesystem` for executor legs).
fn variant_hits(
    index: &CatalogIndex,
    candidates: &[&CatalogEntry],
    query: &DiscoveryQuery,
    structured: Option<&StructuredQuery>,
    executor: &IndexExecutor,
) -> Result<(Vec<Scored>, &'static str), DiscoveryQueryInvalid> {
    let filtered: Vec<&CatalogEntry> = candidates
        .iter()
        .copied()
        .filter(|e| structured.is_none_or(|s| matches_structured(e, s)))
        .collect();
    let hits = match index {
        CatalogIndex::ExactName => (
            filtered
                .into_iter()
                .filter(|e| e.name == query.text)
                .map(|e| (e.surface_id.clone(), None))
                .collect(),
            "kernel",
        ),
        CatalogIndex::StaticAllowlist(ids) => (
            filtered
                .into_iter()
                .filter(|e| {
                    ids.contains(&e.surface_id) && (structured.is_some() || e.name == query.text)
                })
                .map(|e| (e.surface_id.clone(), None))
                .collect(),
            "kernel",
        ),
        CatalogIndex::LexicalRegex => {
            if matches!(query.form, DiscoveryForm::NaturalLanguage) {
                return Err(DiscoveryQueryInvalid::UnsupportedForm);
            }
            let re = crate::regex::Regex::compile(&query.text).map_err(|e| {
                DiscoveryQueryInvalid::InvalidPattern {
                    reason: e.to_string(),
                }
            })?;
            (
                filtered
                    .into_iter()
                    .filter(|e| {
                        search_fields(e)
                            .iter()
                            .any(|(_, _, text)| re.is_match(text))
                    })
                    .map(|e| (e.surface_id.clone(), None))
                    .collect(),
                "kernel",
            )
        }
        CatalogIndex::Bm25 => {
            if matches!(query.form, DiscoveryForm::Regex) {
                return Err(DiscoveryQueryInvalid::UnsupportedForm);
            }
            (
                bm25_rank(&filtered, &query.text)
                    .into_iter()
                    .map(|(e, s)| (e.surface_id.clone(), Some(s)))
                    .collect(),
                "kernel",
            )
        }
        CatalogIndex::Hierarchical => {
            if matches!(query.form, DiscoveryForm::Regex) {
                return Err(DiscoveryQueryInvalid::UnsupportedForm);
            }
            (
                hierarchical_rank(&filtered, &query.text)
                    .into_iter()
                    .map(|(e, s)| (e.surface_id.clone(), Some(s)))
                    .collect(),
                "kernel",
            )
        }
        // The C2 executor legs — a missing leg is `ExecutorUnavailable`
        // (never a fabricated ranking); the form gate still applies
        // (executor legs refuse `regex` per the admissibility table).
        CatalogIndex::Embedding {
            similarity_threshold_ppm,
        } => {
            if matches!(query.form, DiscoveryForm::Regex) {
                return Err(DiscoveryQueryInvalid::UnsupportedForm);
            }
            let Some(exec) = executor.embedding else {
                return Err(DiscoveryQueryInvalid::ExecutorUnavailable);
            };
            let mut hits =
                exec(&filtered, query).map_err(|_| DiscoveryQueryInvalid::ExecutorUnavailable)?;
            hits.retain(|(_, s)| s.is_some_and(|s| s as u64 >= *similarity_threshold_ppm));
            (hits, "provider")
        }
        CatalogIndex::ModelRanked => {
            if matches!(query.form, DiscoveryForm::Regex) {
                return Err(DiscoveryQueryInvalid::UnsupportedForm);
            }
            let Some(exec) = executor.model_ranked else {
                return Err(DiscoveryQueryInvalid::ExecutorUnavailable);
            };
            (
                exec(&filtered, query).map_err(|_| DiscoveryQueryInvalid::ExecutorUnavailable)?,
                "provider",
            )
        }
        CatalogIndex::Filesystem => {
            if matches!(query.form, DiscoveryForm::NaturalLanguage) {
                return Err(DiscoveryQueryInvalid::UnsupportedForm);
            }
            let Some(exec) = executor.filesystem else {
                return Err(DiscoveryQueryInvalid::ExecutorUnavailable);
            };
            (
                exec(&filtered, query).map_err(|_| DiscoveryQueryInvalid::ExecutorUnavailable)?,
                "kernel",
            )
        }
    };
    Ok(hits)
}

/// `query(index, catalog, query, max_reveal)` — the C0/C1 index executor
/// over the whole catalog's non-hidden entries (I-CLOSED). `|hits| ≤
/// min(query.limit, max_reveal_per_search)` (ceiling 32); an empty result is
/// normal. The plan-mode restriction (`hits ⊆ indexed|deferred` members)
/// lives in [`discover`], the `discover_surfaces` lowering.
///
/// Legacy surface-granularity entry — the index is read as a `surface`
/// [`IndexDecl`]; non-surface queries route through
/// [`index_query_decl`]/[`discover_with`].
pub fn index_query(
    index: &CatalogIndex,
    catalog: &Catalog,
    query: &DiscoveryQuery,
    max_reveal_per_search: u64,
) -> Result<DiscoveryResult, DiscoveryQueryInvalid> {
    index_query_decl(
        &IndexDecl {
            index: index.clone(),
            granularity: IndexGranularity::Surface,
        },
        catalog,
        query,
        max_reveal_per_search,
        &IndexExecutor::default(),
    )
}

/// `index_query_decl(decl, catalog, query, max_reveal, executor)` — the
/// R2.8 query path: the `IndexDecl` carries the granularity the index
/// answers at; a query at another granularity is `NotQueryable`, a C2
/// variant without its executor leg is `ExecutorUnavailable`.
pub fn index_query_decl(
    decl: &IndexDecl,
    catalog: &Catalog,
    query: &DiscoveryQuery,
    max_reveal_per_search: u64,
    executor: &IndexExecutor,
) -> Result<DiscoveryResult, DiscoveryQueryInvalid> {
    let candidates: Vec<&CatalogEntry> = catalog.entries.iter().filter(|e| !e.hidden).collect();
    query_over(decl, &candidates, query, max_reveal_per_search, executor)
}

/// `unit_candidates` — aggregate members into the granularity unit a
/// non-surface index answers over: one synthesized entry per namespace /
/// source / catalog (the aggregate's `name` is the unit key; its
/// `search_text` is the member texts' union under their declared fields —
/// an index reads a unit's text only when the member declared the field —
/// I-NARROW). Unit entries are internal candidates — never entries, never
/// revealed; the hit expands back to the members' real `surface_id`s.
fn unit_entries(
    members: &[&CatalogEntry],
    granularity: IndexGranularity,
) -> Vec<(String, CatalogEntry, Vec<String>)> {
    let mut units: BTreeMap<String, Vec<&CatalogEntry>> = BTreeMap::new();
    for e in members {
        let key = match granularity {
            IndexGranularity::Namespace => match &e.namespace {
                Some(ns) => format!("ns:{ns}"),
                None => continue, // a namespace-less entry has no unit at this granularity
            },
            IndexGranularity::Source => format!("src:{}", e.source.as_str()),
            IndexGranularity::Catalog => "catalog".to_string(),
            IndexGranularity::Surface => continue,
        };
        units.entry(key).or_default().push(*e);
    }
    units
        .into_iter()
        .map(|(key, ms)| {
            let mut fields: BTreeSet<SearchTextField> = BTreeSet::new();
            let mut text: BTreeMap<SearchTextField, String> = BTreeMap::new();
            let mut effects: BTreeSet<String> = BTreeSet::new();
            let name_field = if granularity == IndexGranularity::Namespace {
                SearchTextField::NamespaceName
            } else {
                SearchTextField::Name
            };
            fields.insert(name_field);
            for m in &ms {
                for f in &m.index_form.search_text_fields {
                    fields.insert(*f);
                }
                for (f, t) in &m.index_form.search_text {
                    text.entry(*f)
                        .and_modify(|x| {
                            x.push(' ');
                            x.push_str(t);
                        })
                        .or_insert_with(|| t.clone());
                }
                effects.extend(m.effect_summary.iter().cloned());
            }
            // The unit's own name/description text is indexable on its own
            // field — `namespace_name`/`namespace_description` declarations
            // flow from the members; the unit *key* itself is always the
            // `name` field's text.
            text.insert(name_field, {
                let bare = key
                    .strip_prefix("ns:")
                    .or_else(|| key.strip_prefix("src:"))
                    .unwrap_or(&key);
                match text.get(&name_field) {
                    Some(t) => format!("{bare} {t}"),
                    None => bare.to_string(),
                }
            });
            let unit = CatalogEntry {
                surface_id: format!("unit:{key}"),
                capability: String::new(),
                version_id: String::new(),
                source: CatalogSource::Kernel,
                name: key
                    .strip_prefix("ns:")
                    .or_else(|| key.strip_prefix("src:"))
                    .unwrap_or(&key)
                    .to_string(),
                namespace: None,
                admitted_modes: [ExposureMode::Indexed, ExposureMode::Deferred]
                    .into_iter()
                    .collect(),
                pinned: false,
                hidden: false,
                index_form: IndexForm {
                    name: key
                        .strip_prefix("ns:")
                        .or_else(|| key.strip_prefix("src:"))
                        .unwrap_or(&key)
                        .to_string(),
                    title: None,
                    summary_ref: String::new(),
                    namespace: None,
                    tags: Vec::new(),
                    search_text_fields: fields,
                    search_text: text,
                },
                size_tokens: 0,
                estimator_ref: String::new(),
                effect_summary: effects.into_iter().collect(),
                permission_coverage: PermissionCoverage::Unknown,
                label: None,
                availability: Availability::Available,
                is_discovery: false,
                lifted: false,
            };
            let member_ids: Vec<String> = {
                let mut v: Vec<String> = ms.iter().map(|m| m.surface_id.clone()).collect();
                v.sort();
                v
            };
            (key, unit, member_ids)
        })
        .collect()
}

/// The shared query executor — validate, dispatch per variant, bound, rank.
/// `decl.granularity` is the index's declared granularity; a non-surface
/// query aggregates members into unit entries, matches over the units, and
/// expands hits back to member `surface_id`s (each member carries its
/// unit's score; members of a hit unit order by `surface_id`).
fn query_over(
    decl: &IndexDecl,
    candidates: &[&CatalogEntry],
    query: &DiscoveryQuery,
    max_reveal_per_search: u64,
    executor: &IndexExecutor,
) -> Result<DiscoveryResult, DiscoveryQueryInvalid> {
    if decl.granularity != query.granularity {
        return Err(DiscoveryQueryInvalid::NotQueryable {
            index_granularity: decl.granularity,
            query_granularity: query.granularity,
        });
    }
    if query.text.is_empty() {
        return Err(DiscoveryQueryInvalid::Empty);
    }
    if query.text.chars().count() > 4096 {
        return Err(DiscoveryQueryInvalid::TooLong);
    }
    let bound = query
        .limit
        .unwrap_or(u64::MAX)
        .min(max_reveal_per_search)
        .min(32);
    let structured = structured_of(query);
    let (mut hits, executed_by): (Vec<Scored>, &str) =
        if query.granularity == IndexGranularity::Surface {
            variant_hits(&decl.index, candidates, query, structured, executor)?
        } else {
            // Non-surface: structured filters still apply at member level; the
            // index then matches over the surviving units.
            let members: Vec<&CatalogEntry> = candidates
                .iter()
                .copied()
                .filter(|e| structured.is_none_or(|s| matches_structured(e, s)))
                .collect();
            let units = unit_entries(&members, query.granularity);
            let unit_refs: Vec<&CatalogEntry> = units.iter().map(|(_, u, _)| u).collect();
            let (unit_hits, by) = variant_hits(&decl.index, &unit_refs, query, None, executor)?;
            let mut expanded: Vec<Scored> = Vec::new();
            for (uid, score) in unit_hits {
                let Some((_, _, member_ids)) = units.iter().find(|(_, u, _)| u.surface_id == uid)
                else {
                    continue;
                };
                for sid in member_ids {
                    expanded.push((sid.clone(), score));
                }
            }
            (expanded, by)
        };
    // Predicate indexes rank by surface_id; scoring indexes are already
    // ordered by (score desc, surface_id asc).
    if hits.iter().all(|(_, s)| s.is_none()) {
        hits.sort_by(|a, b| a.0.cmp(&b.0));
    }
    let truncated = hits.len() as u64 > bound;
    hits.truncate(bound as usize);
    Ok(DiscoveryResult {
        hits: hits
            .iter()
            .enumerate()
            .map(|(i, (sid, score))| DiscoveryHit {
                surface_id: sid.clone(),
                rank: i as u64 + 1,
                score: score.map(Json::Int),
            })
            .collect(),
        truncated,
        executed_by: executed_by.to_string(),
        cost: None,
    })
}

/// `discover(plan, index, catalog, query, params)` — the kernel lowering of
/// the `discover_surfaces` capability (§5d.3 §2; ADR-0094 D1): the index
/// answers over the plan members whose mode is `indexed` or `deferred`
/// (`hits ⊆ indexed|deferred` — a `direct` or omitted surface is never a
/// discovery hit; a hidden surface is never a candidate).
pub fn discover(
    plan: &ExposurePlan,
    index: &CatalogIndex,
    catalog: &Catalog,
    query: &DiscoveryQuery,
    params: &ExposurePolicyParams,
) -> Result<DiscoveryResult, DiscoveryQueryInvalid> {
    discover_with(
        plan,
        &IndexDecl {
            index: index.clone(),
            granularity: IndexGranularity::Surface,
        },
        catalog,
        query,
        params,
        &IndexExecutor::default(),
    )
}

/// `discover_with(plan, decl, catalog, query, params, executor)` — the
/// decl-carrying `discover` (R2.8): granularity and executor legs are the
/// decl's declaration.
pub fn discover_with(
    plan: &ExposurePlan,
    decl: &IndexDecl,
    catalog: &Catalog,
    query: &DiscoveryQuery,
    params: &ExposurePolicyParams,
    executor: &IndexExecutor,
) -> Result<DiscoveryResult, DiscoveryQueryInvalid> {
    let discoverable: std::collections::BTreeSet<&str> = plan
        .entries
        .iter()
        .filter(|(_, m)| matches!(m, ExposureMode::Indexed | ExposureMode::Deferred))
        .map(|(s, _)| s.as_str())
        .collect();
    let candidates: Vec<&CatalogEntry> = catalog
        .entries
        .iter()
        .filter(|e| !e.hidden && discoverable.contains(e.surface_id.as_str()))
        .collect();
    query_over(
        decl,
        &candidates,
        query,
        params.max_reveal_per_search,
        executor,
    )
}

/// `index_forms_at(catalog, index, granularity)` — the §5d.2.8 producer:
/// the form kinds `index` serves at `granularity`. An index bound in
/// `catalog.indexes` answers only at its declared granularity (a mismatch
/// is `NotQueryable`); an unbound index answers at `surface` only (the
/// legacy per-entry read).
pub fn index_forms_at(
    catalog: &Catalog,
    index: &CatalogIndex,
    granularity: IndexGranularity,
) -> Result<BTreeSet<DiscoveryFormKind>, DiscoveryQueryInvalid> {
    match catalog.indexes.iter().find(|d| d.index == *index) {
        Some(d) if d.granularity == granularity => Ok(form_kinds(&d.index)),
        Some(d) => Err(DiscoveryQueryInvalid::NotQueryable {
            index_granularity: d.granularity,
            query_granularity: granularity,
        }),
        None if granularity == IndexGranularity::Surface => Ok(form_kinds(index)),
        None => Err(DiscoveryQueryInvalid::NotQueryable {
            index_granularity: IndexGranularity::Surface,
            query_granularity: granularity,
        }),
    }
}

// ── Catalog deltas / epochs (schemas — ADR-0095 D1/D2) ───────────────────────

/// `sync_source`'s trigger sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncTrigger {
    /// `notifications/tools/list_changed`.
    ListChanged,
    /// `ttlMs` expiry.
    TtlExpired,
    /// A reconnect.
    Reconnect,
    /// `extension_enabled`.
    ExtensionEnabled,
    /// `extension_disabled`.
    ExtensionDisabled,
    /// `authorization_changed`.
    AuthorizationChanged,
}

/// `CatalogDelta{source_ref, cause, added[], removed[], changed[],
/// index_decls[]}` — every delta is emitted as `action.tool.catalog.delta`,
/// including empty ones. `index_decls` are the index declarations the
/// source's new listing re-declares (R2.8 — a source may re-declare the
/// granularities it indexes at; an adoption refusing them is
/// `CatalogDriftError::UnsupportedIndexGranularity`).
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogDelta {
    /// The source.
    pub source_ref: String,
    /// The trigger.
    pub cause: SyncTrigger,
    /// Added entries.
    pub added: Vec<CatalogEntry>,
    /// Removed surface ids.
    pub removed: Vec<String>,
    /// Changed entries (re-lowered).
    pub changed: Vec<CatalogEntry>,
    /// The source's re-declared index set (empty = the source declares
    /// nothing new — no granularity check).
    pub index_decls: Vec<IndexDecl>,
}

/// `CatalogEpoch{epoch, catalog_id_prev, catalog_id, bundle_delta,
/// loss_report, adopted_by}` — `action.tool.catalog.epoch`, appended to the
/// run manifest's `catalog_epochs[]`.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogEpoch {
    /// The new epoch number.
    pub epoch: u64,
    /// The prior `catalog_id`.
    pub catalog_id_prev: String,
    /// The new `catalog_id`.
    pub catalog_id: String,
    /// The incremental bundle delta (the adopted lowering).
    pub bundle_delta: Json,
    /// The lowering loss report.
    pub loss_report: Option<Json>,
    /// Who adopted it.
    pub adopted_by: String,
}

impl CatalogDelta {
    /// The canonical payload form (the `action.tool.catalog.delta` body and
    /// the `bundle_delta` member of `CatalogEpoch`): added/changed entries
    /// are *surface descriptors*, never definitions, and carry the ADR-0021
    /// lowering stamp `authority = unverified` (ADR-0095 D2 — adopted
    /// surfaces are lifted, never coerced).
    pub fn to_json(&self) -> Json {
        let entry_json = |e: &CatalogEntry| {
            Json::obj([
                ("surface_id", Json::str(e.surface_id.clone())),
                ("capability", Json::str(e.capability.clone())),
                ("version_id", Json::str(e.version_id.clone())),
                ("name", Json::str(e.name.clone())),
                ("authority", Json::str("unverified")),
                // The full descriptor rides beside the projection — the
                // durable-fold rebuild (`hh_control`'s resume fold) reads
                // `entry`, the spec's member minimum stays.
                ("entry", catalog_entry_json(e)),
            ])
        };
        Json::obj([
            ("source_ref", Json::str(self.source_ref.clone())),
            (
                "cause",
                Json::str(match self.cause {
                    SyncTrigger::ListChanged => "list_changed",
                    SyncTrigger::TtlExpired => "ttl_expired",
                    SyncTrigger::Reconnect => "reconnect",
                    SyncTrigger::ExtensionEnabled => "extension_enabled",
                    SyncTrigger::ExtensionDisabled => "extension_disabled",
                    SyncTrigger::AuthorizationChanged => "authorization_changed",
                }),
            ),
            (
                "added",
                Json::Arr(self.added.iter().map(entry_json).collect()),
            ),
            (
                "removed",
                Json::Arr(self.removed.iter().map(|s| Json::str(s.clone())).collect()),
            ),
            (
                "changed",
                Json::Arr(self.changed.iter().map(entry_json).collect()),
            ),
            (
                "index_decls",
                Json::Arr(self.index_decls.iter().map(index_decl_json).collect()),
            ),
        ])
    }
}

/// `catalog_delta(catalog, source_ref, cause, listing)` — the catalog-local
/// half of `sync_source` (§5d.3 §2; ADR-0095 D1): given a source's *new*
/// listing (already-lifted entries — the MCP edge that produces the listing
/// is R-2.5.4/S3.9), compute `{added, removed, changed}` against the
/// catalog's current entries for `source_ref`. `changed` compares the
/// lowered entry modulo `availability`/`permission_coverage` (run-state
/// fields, not listing data). Every delta — including empty — is emitted as
/// `action.tool.catalog.delta` by the caller.
pub fn catalog_delta(
    catalog: &Catalog,
    source_ref: &str,
    cause: SyncTrigger,
    listing: &[CatalogEntry],
) -> CatalogDelta {
    let current: BTreeMap<&str, &CatalogEntry> = catalog
        .entries
        .iter()
        .filter(|e| e.source.as_str() == source_ref)
        .map(|e| (e.surface_id.as_str(), e))
        .collect();
    let mut added = Vec::new();
    let mut changed = Vec::new();
    let mut removed = Vec::new();
    let new_ids: BTreeSet<&str> = listing.iter().map(|e| e.surface_id.as_str()).collect();
    for e in listing {
        match current.get(e.surface_id.as_str()) {
            None => added.push(e.clone()),
            Some(old) => {
                let mut cmp = (*old).clone();
                cmp.availability = e.availability.clone();
                cmp.permission_coverage = e.permission_coverage;
                if cmp != *e {
                    changed.push(e.clone());
                }
            }
        }
    }
    for (sid, _) in current.iter() {
        if !new_ids.contains(sid) {
            removed.push(sid.to_string());
        }
    }
    removed.sort();
    CatalogDelta {
        source_ref: source_ref.to_string(),
        cause,
        added,
        removed,
        changed,
        index_decls: Vec::new(),
    }
}

/// `catalog_delta_with(…, index_decls)` — the decl-carrying delta (R2.8):
/// the source's re-declared index set rides the delta to `adopt` (an
/// adoption serving a granularity the catalog does not declare is
/// `CatalogDriftError::UnsupportedIndexGranularity`).
pub fn catalog_delta_with(
    catalog: &Catalog,
    source_ref: &str,
    cause: SyncTrigger,
    listing: &[CatalogEntry],
    index_decls: Vec<IndexDecl>,
) -> CatalogDelta {
    let mut d = catalog_delta(catalog, source_ref, cause, listing);
    d.index_decls = index_decls;
    d
}

/// `adopt`'s refusal — `CatalogDriftRefused | unsupported_index_granularity
/// | pin_binds_lifted` (§5d.3 §2). `ask` is the C2 `permission_request`
/// path (ADR-0095 D2; R2.8 lands the driver half — the kernel's typed
/// refusal stays `Refused` until the ask resolves).
#[derive(Debug, Clone, PartialEq)]
pub enum CatalogDriftError {
    /// `drift_policy = ask` — a permission request is required (C2).
    Refused,
    /// The delta's `index_decls` declare a granularity the catalog does
    /// not serve (`index_decls` non-empty ⇒ every declared granularity
    /// must be bound in `catalog.indexes` — §5d.3's error column).
    UnsupportedIndexGranularity {
        /// The unsupported granularity.
        granularity: IndexGranularity,
    },
    /// A delta would adopt a `pinned ∧ lifted` entry — a pin binds an
    /// admission, never an unverified lift (the adopt-side twin of
    /// `CatalogBuildError::PinBindsLifted`).
    PinBindsLifted {
        /// The surface.
        surface_id: String,
    },
}

impl std::fmt::Display for CatalogDriftError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CatalogDriftError::Refused => write!(f, "CatalogDriftRefused"),
            CatalogDriftError::UnsupportedIndexGranularity { granularity } => {
                write!(f, "unsupported_index_granularity: {}", granularity.as_str())
            }
            CatalogDriftError::PinBindsLifted { surface_id } => {
                write!(f, "pin_binds_lifted: {surface_id}")
            }
        }
    }
}

impl std::error::Error for CatalogDriftError {}

/// The `adopt` result (§5d.3 §2; ADR-0095 D2).
#[derive(Debug, Clone, PartialEq)]
pub enum AdoptOutcome {
    /// `freeze` — `removed` entries become `availability =
    /// unavailable(removed)`; `added`/`changed` are recorded on the delta
    /// but are never entries; `catalog_id`, `epoch` and
    /// `configuration_version_id` are unchanged (AC-R-2.5.3-8's freeze
    /// half).
    Frozen {
        /// The catalog with `removed` marked `unavailable(removed)`.
        catalog: Catalog,
    },
    /// `adopt` — the delta lowers into a new epoch: `removed` entries drop,
    /// `added`/`changed` entries join as catalog members (their lowering
    /// stamps `authority = unverified` on the `bundle_delta` descriptors),
    /// `catalog_id` is recomputed and `epoch` increments.
    Adopted {
        /// The new-epoch catalog.
        catalog: Catalog,
        /// The `action.tool.catalog.epoch` record.
        epoch: CatalogEpoch,
    },
}

/// `AdoptInputs` — the C1 incremental-lowering inputs `adopt` consumes
/// (§5d.3 §2; ADR-0095 D2): the `LoweringLossReport` the delta's lift
/// produced (populated on `CatalogEpoch.loss_report`, never invented —
/// `None` only when the lift declared no loss) and the coverage oracle
/// recomputing `permission_coverage` on adopted entries. The compiler
/// never resolves grants (I-CLOSED — coverage is the caller's verdict;
/// `None` keeps the listing-stamped coverage).
#[derive(Default)]
pub struct AdoptInputs<'a> {
    /// The adopted lowering's `LoweringLossReport` (target `catalog`).
    pub loss_report: Option<crate::lcd::LoweringLossReport>,
    /// `permission_coverage` recompute — `entry → coverage` applied to
    /// every added/changed entry *after* it joins the catalog (ADR-0095
    /// D2: "typically `uncovered` until a grant exists").
    pub coverage: Option<&'a dyn Fn(&CatalogEntry) -> PermissionCoverage>,
}

/// `adopt(catalog, delta, drift_policy, adopted_by)` (§5d.3 §2; ADR-0095
/// D2) — the C1 signature; delegates to [`adopt_with`] with no incremental
/// inputs (the listing-only path: no declared loss, listing-stamped
/// coverage).
pub fn adopt(
    catalog: &Catalog,
    delta: &CatalogDelta,
    drift_policy: DriftPolicy,
    adopted_by: &str,
) -> Result<AdoptOutcome, CatalogDriftError> {
    adopt_with(
        catalog,
        delta,
        drift_policy,
        adopted_by,
        &AdoptInputs::default(),
    )
}

/// `adopt_with(catalog, delta, drift_policy, adopted_by, inputs)` — the
/// incremental-lowering `adopt`: on `adopt` the delta's added/changed
/// entries join with `permission_coverage` recomputed through the declared
/// oracle and the epoch record carries the lowering's `loss_report`
/// (both folded into `action.tool.catalog.epoch` — CC3: a lift that loses
/// declares it on the epoch, never silently).
pub fn adopt_with(
    catalog: &Catalog,
    delta: &CatalogDelta,
    drift_policy: DriftPolicy,
    adopted_by: &str,
    inputs: &AdoptInputs,
) -> Result<AdoptOutcome, CatalogDriftError> {
    match drift_policy {
        DriftPolicy::Ask => Err(CatalogDriftError::Refused),
        DriftPolicy::Freeze => {
            let mut out = catalog.clone();
            let removed: BTreeSet<&str> = delta.removed.iter().map(|s| s.as_str()).collect();
            for e in out.entries.iter_mut() {
                if removed.contains(e.surface_id.as_str()) {
                    e.availability = Availability::Unavailable("removed".to_string());
                }
            }
            // I-EPOCH: `catalog_id` is unchanged under freeze — the id is
            // over the entry *set* (surface ids + epoch + bundle), not
            // run-state fields.
            debug_assert_eq!(out.catalog_id, catalog.catalog_id);
            Ok(AdoptOutcome::Frozen { catalog: out })
        }
        DriftPolicy::Adopt => {
            // The C2 drift checks (R2.8) — typed refusals, never a silent
            // drop:
            // - a re-declared index granularity the catalog does not serve
            //   is `UnsupportedIndexGranularity`;
            // - an adopted `pinned ∧ lifted` entry is `PinBindsLifted`.
            if !delta.index_decls.is_empty() {
                let served: BTreeSet<IndexGranularity> = catalog
                    .indexes
                    .iter()
                    .map(|d| d.granularity)
                    .chain(std::iter::once(IndexGranularity::Surface))
                    .collect();
                for d in &delta.index_decls {
                    if !served.contains(&d.granularity) {
                        return Err(CatalogDriftError::UnsupportedIndexGranularity {
                            granularity: d.granularity,
                        });
                    }
                }
            }
            for e in delta.changed.iter().chain(delta.added.iter()) {
                if e.pinned && e.lifted {
                    return Err(CatalogDriftError::PinBindsLifted {
                        surface_id: e.surface_id.clone(),
                    });
                }
            }
            let removed: BTreeSet<&str> = delta.removed.iter().map(|s| s.as_str()).collect();
            let mut entries: Vec<CatalogEntry> = catalog
                .entries
                .iter()
                .filter(|e| !removed.contains(e.surface_id.as_str()))
                .cloned()
                .collect();
            for e in delta.changed.iter().chain(delta.added.iter()) {
                match entries.iter_mut().find(|x| x.surface_id == e.surface_id) {
                    Some(slot) => *slot = e.clone(),
                    None => entries.push(e.clone()),
                }
            }
            // `permission_coverage` recomputes on the adopted entries —
            // the caller's oracle is the only coverage authority (the
            // compiler never resolves grants; a missing oracle keeps the
            // listing stamp, declared, never widened).
            if let Some(coverage) = inputs.coverage {
                let adopted: BTreeSet<&str> = delta
                    .added
                    .iter()
                    .chain(delta.changed.iter())
                    .map(|e| e.surface_id.as_str())
                    .collect();
                for e in entries.iter_mut() {
                    if adopted.contains(e.surface_id.as_str()) {
                        e.permission_coverage = coverage(e);
                    }
                }
            }
            entries.sort_by(|a, b| a.surface_id.cmp(&b.surface_id));
            let new_epoch = catalog.epoch + 1;
            let new_id = catalog_id_of(&entries, new_epoch, &catalog.bundle_id);
            let epoch = CatalogEpoch {
                epoch: new_epoch,
                catalog_id_prev: catalog.catalog_id.clone(),
                catalog_id: new_id.clone(),
                bundle_delta: delta.to_json(),
                loss_report: inputs.loss_report.as_ref().map(crate::schema::loss_json),
                adopted_by: adopted_by.to_string(),
            };
            Ok(AdoptOutcome::Adopted {
                catalog: Catalog {
                    catalog_id: new_id,
                    epoch: new_epoch,
                    bundle_id: catalog.bundle_id.clone(),
                    sources: catalog.sources.clone(),
                    entries,
                    indexes: catalog.indexes.clone(),
                    index_ref: None, // index rebuilt — `action.tool.catalog.built` is the caller's row
                },
                epoch,
            })
        }
    }
}

// ── sync_source (§5d.3 §2; R-2.5.4⁰ — ticket S3.9) ───────────────────────────

/// The `sync_source` result: the emitted `action.tool.catalog.delta`
/// payload (always produced — empty deltas included), the
/// drift-policy outcome and, on `adopt`, the `action.tool.catalog.epoch`
/// payload. The caller appends both rows to the run ledger.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncOutcome {
    /// The computed delta.
    pub delta: CatalogDelta,
    /// The `action.tool.catalog.delta` payload (`delta.to_json()`).
    pub delta_event: Json,
    /// `Frozen`/`Adopted` (or `Refused` under `drift_policy = ask`) —
    /// the source row's `snapshot_hash`/`ttl`/`listened` are updated on
    /// either catalog (`catalog_id` is over the entry set, so a source
    /// refresh never rewrites it under freeze — I-EPOCH).
    pub outcome: Result<AdoptOutcome, CatalogDriftError>,
    /// The `action.tool.catalog.epoch` payload, when adopted.
    pub epoch_event: Option<Json>,
}

/// `sync_source(catalog, source_ref, cause, listing, source_state,
/// drift_policy, adopted_by)` (§5d.3 §2): all six `SyncTrigger`s drive
/// the same path — `catalog_delta` against the source's current entries,
/// then `adopt`/`freeze`. The listing arrives already lowered to
/// `CatalogEntry`s (the MCP edge's `import_listing`/`lift` half is
/// R-2.5.4/S3.9's `hh-mcp` + `hh-registry` work). `source_state` is the
/// post-refresh row (`snapshot_hash` = the new `listing_hash`).
///
/// Every call emits `action.tool.catalog.delta` — including the empty
/// delta (a refresh that changes nothing is still a synchronization
/// record).
pub fn sync_source(
    catalog: &Catalog,
    source_ref: &str,
    cause: SyncTrigger,
    listing: &[CatalogEntry],
    source_state: &SourceState,
    drift_policy: DriftPolicy,
    adopted_by: &str,
) -> SyncOutcome {
    sync_source_with(
        catalog,
        source_ref,
        cause,
        listing,
        source_state,
        drift_policy,
        adopted_by,
        Vec::new(),
    )
}

/// `sync_source_with(…, index_decls)` — the decl-carrying sync (R2.8): the
/// source's re-declared index set rides the delta (`adopt` refuses an
/// unsupported granularity — `CatalogDriftError::UnsupportedIndexGranularity`).
#[allow(clippy::too_many_arguments)]
pub fn sync_source_with(
    catalog: &Catalog,
    source_ref: &str,
    cause: SyncTrigger,
    listing: &[CatalogEntry],
    source_state: &SourceState,
    drift_policy: DriftPolicy,
    adopted_by: &str,
    index_decls: Vec<IndexDecl>,
) -> SyncOutcome {
    let delta = catalog_delta_with(catalog, source_ref, cause, listing, index_decls);
    let delta_event = delta.to_json();
    let outcome = adopt(catalog, &delta, drift_policy, adopted_by);
    let epoch_event = match &outcome {
        Ok(AdoptOutcome::Adopted { epoch, .. }) => Some(catalog_epoch_payload(epoch)),
        _ => None,
    };
    // Update the source row on whichever catalog survived (freeze keeps
    // `catalog_id`; the row is run state, not identity).
    let outcome = outcome.map(|o| {
        let (mut catalog, epoch) = match o {
            AdoptOutcome::Frozen { catalog } => (catalog, None),
            AdoptOutcome::Adopted { catalog, epoch } => (catalog, Some(epoch)),
        };
        match catalog
            .sources
            .iter_mut()
            .find(|s| s.source_ref == source_ref)
        {
            Some(row) => *row = source_state.clone(),
            None => catalog.sources.push(source_state.clone()),
        }
        match epoch {
            Some(epoch) => AdoptOutcome::Adopted { catalog, epoch },
            None => AdoptOutcome::Frozen { catalog },
        }
    });
    SyncOutcome {
        delta,
        delta_event,
        outcome,
        epoch_event,
    }
}

// ── Errors (the spec's error column) ─────────────────────────────────────────

/// `CatalogBuildError` — `source_unavailable | IndexOverflow{pages | bytes}
/// | pin_binds_lifted` (R2.8's attestation leg).
#[derive(Debug, Clone, PartialEq)]
pub enum CatalogBuildError {
    /// A declared source could not be read.
    SourceUnavailable {
        /// The source.
        source_ref: String,
    },
    /// The listing exceeded the page/byte ceilings or repeated a cursor.
    IndexOverflow {
        /// `pages` | `bytes` | `duplicate_cursor`.
        what: String,
    },
    /// `lifted ∧ pinned` — a pin binds the admission of a *compiled*
    /// surface; pinning an unverified lift is `pin_binds_lifted` (§5d.3
    /// error column — never an attestation bypass).
    PinBindsLifted {
        /// The surface.
        surface_id: String,
    },
}

/// `select_surfaces` errors — `NoDiscoverySurface | ExposureBudgetExceeded |
/// UnexpressibleExposure{mode, profile} | PolicyViolation`.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectError {
    /// I-DISCOVERY: the plan defers (or indexes at granularity ≠ `surface`)
    /// with no `direct` discovery capability.
    NoDiscoverySurface,
    /// I-BUDGET: pinned surfaces alone exceed `direct_token_share ×
    /// window_cap` — refused, never silently demoted.
    ExposureBudgetExceeded {
        /// The pinned requirement in tokens.
        required: u64,
        /// The budget in tokens.
        cap: u64,
    },
    /// A mode the profile cannot express.
    UnexpressibleExposure {
        /// The mode.
        mode: String,
        /// The profile.
        profile: String,
    },
    /// I-NARROW: the policy returned a mode outside `admitted_modes`, named a
    /// hidden/unavailable surface, dropped a pinned entry's `direct` mode, or
    /// named the same surface twice (I-ONE-MODE).
    PolicyViolation {
        /// What the policy did.
        detail: String,
    },
}

impl std::fmt::Display for SelectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SelectError::NoDiscoverySurface => write!(f, "NoDiscoverySurface"),
            SelectError::ExposureBudgetExceeded { required, cap } => {
                write!(f, "ExposureBudgetExceeded: {required} > {cap}")
            }
            SelectError::UnexpressibleExposure { mode, profile } => {
                write!(f, "UnexpressibleExposure({mode}, {profile})")
            }
            SelectError::PolicyViolation { detail } => {
                write!(f, "PolicyViolation: {detail}")
            }
        }
    }
}

impl std::error::Error for SelectError {}

/// `check_callable` refusals — `SurfaceNotRevealed{surface_id?, hint}` (the
/// refusal never carries a definition) or `SurfaceUnavailable{reason}`;
/// recorded `action.tool.call.refused`.
#[derive(Debug, Clone, PartialEq)]
pub enum CallRefusal {
    /// The surface was not `direct` in the producing call's plan
    /// (I-CALLABLE — `callable ⇔ revealed`).
    SurfaceNotRevealed {
        /// The surface id, when the name resolved in the catalog.
        surface_id: Option<String>,
        /// `search` when the surface exists deferred/indexed (discovery could
        /// find it); `none` when the name is unknown.
        hint: RevealHint,
    },
    /// The surface is `unavailable`/`stale`.
    SurfaceUnavailable {
        /// The reason.
        reason: String,
    },
}

/// `hint ∈ {search, none}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevealHint {
    /// The surface is discoverable (`indexed`/`deferred` in the catalog).
    Search,
    /// No such surface.
    None,
}

/// `reveal`'s refusal — I-CLOSED/I-EPOCH: only catalog members of the current
/// epoch may be revealed.
#[derive(Debug, Clone, PartialEq)]
pub enum RevealError {
    /// The surface id is not a catalog member (`IndexStale` for a stale-epoch
    /// id is the same refusal — an epoch membership test, never a definition).
    NotInCatalog {
        /// The surface.
        surface_id: String,
    },
}

// ── build_catalog (ADR-0093 D2) ──────────────────────────────────────────────

/// `build_catalog(bundle, sources_state, epoch)` — one `CatalogEntry` per
/// compiled `SurfaceBinding` in the bundle's `RuntimePlan.tools[]`; surfaces
/// without compiled identity are never entries (I-CLOSED — a `ToolBinding`
/// with no `surface` produces no entry, and the capability is reported
/// `unexposed` by the caller). `hidden` entries carry `hidden` and are
/// excluded from `index_ref` query results by construction.
///
/// `sources_state` maps capability `semantic_id → CatalogSource` (the
/// declared source); absent ⇒ `Harness(None)` — the C0-native default.
/// `size_tokens` is `canonical-bytes/4` under the `bytes_div_4` estimator —
/// declared, never measured (T-LCD-15 honesty for an estimate).
pub fn build_catalog(
    bundle: &CompiledBundle,
    sources_state: &BTreeMap<String, CatalogSource>,
    epoch: u64,
    source_states: Vec<SourceState>,
) -> Result<Catalog, CatalogBuildError> {
    build_catalog_with(
        bundle,
        sources_state,
        epoch,
        source_states,
        &BTreeSet::new(),
    )
}

/// `build_catalog_with(…, lifted)` — the attestation-aware build (R2.8):
/// `lifted` names the `surface_id`s whose capability arrived through an
/// unverified lift (MCP/extension import). The entries carry `lifted`; a
/// `lifted ∧ pinned` surface is `CatalogBuildError::PinBindsLifted` — a
/// pin binds an admission, never an unverified lift.
pub fn build_catalog_with(
    bundle: &CompiledBundle,
    sources_state: &BTreeMap<String, CatalogSource>,
    epoch: u64,
    source_states: Vec<SourceState>,
    lifted: &BTreeSet<String>,
) -> Result<Catalog, CatalogBuildError> {
    let mut entries = Vec::new();
    for tool in &bundle.runtime_plan.tools {
        let Some(binding) = &tool.surface else {
            continue;
        };
        let mut entry = entry_from_binding(bundle, tool, binding, sources_state);
        if lifted.contains(entry.surface_id.as_str()) {
            entry.lifted = true;
            if entry.pinned {
                return Err(CatalogBuildError::PinBindsLifted {
                    surface_id: entry.surface_id.clone(),
                });
            }
        }
        entries.push(entry);
    }
    entries.sort_by(|a, b| a.surface_id.cmp(&b.surface_id));
    let catalog_id = catalog_id_of(&entries, epoch, &bundle.bundle_id);
    Ok(Catalog {
        catalog_id,
        epoch,
        bundle_id: bundle.bundle_id.clone(),
        sources: source_states,
        entries,
        indexes: Vec::new(),
        index_ref: None,
    })
}

fn entry_from_binding(
    bundle: &CompiledBundle,
    tool: &crate::plan::ToolBinding,
    binding: &SurfaceBinding,
    sources_state: &BTreeMap<String, CatalogSource>,
) -> CatalogEntry {
    let summary_ref = hh_identity::idp::idp_id(
        "hir.text",
        format!("summary of {}", binding.surface_name).as_bytes(),
    );
    let _ = bundle;
    CatalogEntry {
        surface_id: binding.surface_id.clone(),
        capability: binding.capability_ref.semantic_id.clone(),
        version_id: binding.capability_ref.version_id.clone(),
        source: sources_state
            .get(&binding.capability_ref.semantic_id)
            .cloned()
            .unwrap_or(CatalogSource::Harness(None)),
        name: binding.surface_name.clone(),
        namespace: None,
        admitted_modes: binding.admitted_modes.clone(),
        pinned: binding.pinned,
        hidden: binding.hidden,
        index_form: IndexForm {
            name: binding.surface_name.clone(),
            title: None,
            summary_ref,
            namespace: None,
            tags: Vec::new(),
            search_text_fields: [SearchTextField::Name, SearchTextField::Description]
                .into_iter()
                .collect(),
            // No field text is materialized at `build_catalog` — the
            // capability's `purpose`/`Text` leaves live behind refs the
            // compiler does not resolve (I-CLOSED). A lowering or adoption
            // step that materializes them fills `search_text`.
            search_text: BTreeMap::new(),
        },
        size_tokens: surface_size_tokens(binding),
        estimator_ref: "bytes_div_4".to_string(),
        effect_summary: binding.effects_bound.clone(),
        permission_coverage: PermissionCoverage::Unknown,
        label: None,
        availability: Availability::Available,
        is_discovery: tool_is_discovery(tool),
        lifted: false,
    }
}

/// The `bytes_div_4` estimator — canonical surface bytes ÷ 4 (declared, never
/// measured).
fn surface_size_tokens(b: &SurfaceBinding) -> u64 {
    (crate::schema::arg_map_json_pub(&b.arg_map)
        .to_canonical_string()
        .len() as u64
        + b.surface_name.len() as u64)
        / 4
}

/// Whether a `ToolBinding` is the `discover_surfaces` capability — read from
/// the bundle's capability record when carried (`exposure_hint.discovery`);
/// the binding carries no hint of its own.
fn tool_is_discovery(tool: &crate::plan::ToolBinding) -> bool {
    matches!(
        tool.observation_contract.get("discovery"),
        Some(Json::Bool(true))
    ) || tool_is_discovery_by_name(tool)
}

/// The name fallback — the kernel mints `discover_surfaces` under the
/// reserved `hh.` namespace (the resolver adds it; the name is the C0 marker).
fn tool_is_discovery_by_name(tool: &crate::plan::ToolBinding) -> bool {
    tool.surface
        .as_ref()
        .is_some_and(|b| b.surface_name == "discover_surfaces")
}

/// `catalog_id = H(canonical(entries by surface_id) ∥ epoch ∥ bundle_id)` —
/// I-EPOCH: the id changes only through `adopt`.
pub fn catalog_id_of(entries: &[CatalogEntry], epoch: u64, bundle_id: &str) -> String {
    let ids: Vec<Json> = entries
        .iter()
        .map(|e| Json::str(e.surface_id.clone()))
        .collect();
    let body = Json::obj([
        ("bundle_id", Json::str(bundle_id)),
        ("entries", Json::Arr(ids)),
        ("epoch", Json::Int(epoch as i64)),
    ]);
    hh_identity::idp::idp_id("catalog", body.to_canonical_string().as_bytes())
}

// ── select_surfaces (ADR-0093 D3/D4/D8) ──────────────────────────────────────

/// The `tool_exposure_policy` contract — `policy.rank(entries, turn_state,
/// params) → [(surface_id, mode)]`. The kernel runs `admit` before and
/// `enforce` after; a violation is `PolicyViolation`, never a delivery.
pub trait ExposurePolicy {
    /// `rank` — the [(surface_id, mode)] selection over the admissible set
    /// (the kernel passes each entry's *narrowed* admitted modes).
    fn rank(
        &self,
        entries: &[CatalogEntry],
        admitted: &BTreeMap<String, BTreeSet<ExposureMode>>,
        turn_state: &TurnState,
        params: &ExposurePolicyParams,
    ) -> Vec<(String, ExposureMode)>;
}

/// `direct_all` — the kernel's absent-policy selection: every admitted
/// non-hidden, available surface `direct` (AC-R-2.5.3-12).
pub fn direct_all(
    entries: &[CatalogEntry],
    admitted: &BTreeMap<String, BTreeSet<ExposureMode>>,
) -> Vec<(String, ExposureMode)> {
    entries
        .iter()
        .filter(|e| {
            !e.hidden
                && e.availability == Availability::Available
                && admitted
                    .get(&e.surface_id)
                    .is_some_and(|m| m.contains(&ExposureMode::Direct))
        })
        .map(|e| (e.surface_id.clone(), ExposureMode::Direct))
        .collect()
}

/// `SelectionGates` — the selection-time capability gates (R2.8 C2 legs,
/// §5d.2's gating inputs). Every member is a *declared* gate — `false` is
/// the absence of the gate, never a guess.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SelectionGates {
    /// `index_unverified` — the verifier-attestation gate for indexing
    /// unverified-lifted surfaces (§5d.3's attestation leg): a `lifted`
    /// entry's `indexed` mode is admissible **only** under this gate.
    /// `false` ⇒ `indexed` is dropped from a lifted entry's admit meet
    /// (`deferred`/`direct` still admit — the gate gates *indexing*, not
    /// delivery; an entry left with no mode is `omitted(ModeUnsupportedByProfile)`).
    /// `lifted ∧ pinned` never reaches here — the build refuses it
    /// (`CatalogBuildError::PinBindsLifted`).
    pub index_unverified: bool,
}

/// `select_surfaces(catalog, turn_state, profile_modes, params, policy)` —
/// the kernel `admit → policy.rank → enforce` pipeline (§5d.3 §2):
///
/// - **admit**: `hidden` → omitted(Hidden); `availability ≠ available` →
///   omitted(Unavailable); `admitted = definition ∩ profile ∩ policy`
///   (I-NARROW — policy narrows via `default_mode_by_source` and
///   `pinned_direct`); empty meet → omitted(ModeUnsupportedByProfile); pinned
///   and revealed surfaces are `direct` by construction.
/// - **rank**: `policy.rank` or `direct_all` when no policy is bound.
/// - **enforce**: I-NARROW (a mode outside the meet, a hidden/unavailable
///   entry, a dropped pinned entry, a duplicated surface ⇒ `PolicyViolation`);
///   I-ONE-MODE; I-DISCOVERY (`NoDiscoverySurface`); I-BUDGET (pinned overage
///   ⇒ `ExposureBudgetExceeded`; other overage ⇒ omitted(Budget)); I-ORDER
///   (the order extends `turn_state.prior_order` prefix-wise).
pub fn select_surfaces(
    catalog: &Catalog,
    turn_state: &TurnState,
    profile_modes: &BTreeSet<ExposureMode>,
    params: &ExposurePolicyParams,
    policy: Option<&dyn ExposurePolicy>,
) -> Result<ExposurePlan, SelectError> {
    select_surfaces_with(
        catalog,
        turn_state,
        profile_modes,
        params,
        policy,
        &SelectionGates::default(),
    )
}

/// `select_surfaces_with(…, gates)` — the gated `select_surfaces` (R2.8):
/// the `permission_gate` and `index_unverified` legs run in the kernel
/// admit (the rows they produce ride the plan's `omitted[]` — never silent
/// drops):
///
/// - `permission_gate = hide_uncovered` ⇒ `uncovered` entries omit
///   `PermissionUncovered` (the C2 half of OQ-234);
/// - `permission_gate = demote_uncovered` ⇒ `uncovered` entries lose
///   `direct` from the meet (they may still serve `indexed`/`deferred` —
///   demoted, never hidden);
/// - `gates.index_unverified = false` ⇒ a `lifted` entry loses `indexed`
///   from the meet (the attestation gate's absence leg).
pub fn select_surfaces_with(
    catalog: &Catalog,
    turn_state: &TurnState,
    profile_modes: &BTreeSet<ExposureMode>,
    params: &ExposurePolicyParams,
    policy: Option<&dyn ExposurePolicy>,
    gates: &SelectionGates,
) -> Result<ExposurePlan, SelectError> {
    let mut omitted: Vec<OmittedEntry> = Vec::new();
    let mut admitted: BTreeMap<String, BTreeSet<ExposureMode>> = BTreeMap::new();
    let revealed_ids: BTreeSet<&str> = turn_state
        .revealed
        .entries
        .iter()
        .map(|e| e.surface_id.as_str())
        .collect();
    let mut forced_direct: BTreeSet<String> = BTreeSet::new();

    // Kernel admit.
    for e in &catalog.entries {
        if e.hidden {
            omitted.push(OmittedEntry {
                surface_id: e.surface_id.clone(),
                reason: OmitReason::Hidden,
            });
            continue;
        }
        if e.availability != Availability::Available {
            omitted.push(OmittedEntry {
                surface_id: e.surface_id.clone(),
                reason: OmitReason::Unavailable,
            });
            continue;
        }
        // `permission_gate` (OQ-234) — `hide_uncovered` removes the entry
        // wholesale; `demote_uncovered` drops `direct` from the meet (the
        // C1 default); `none` reads no coverage.
        if params.permission_gate == PermissionGate::HideUncovered
            && e.permission_coverage == PermissionCoverage::Uncovered
        {
            omitted.push(OmittedEntry {
                surface_id: e.surface_id.clone(),
                reason: OmitReason::PermissionUncovered,
            });
            continue;
        }
        // I-NARROW: definition ∩ profile ∩ policy. `default_mode_by_source`
        // narrows to its singleton when it names this source.
        let mut meet: BTreeSet<ExposureMode> = e
            .admitted_modes
            .intersection(profile_modes)
            .copied()
            .collect();
        // `demote_uncovered` — a coverage-less entry keeps its non-direct
        // modes (the pin check below fails it if `direct` was mandatory).
        if params.permission_gate == PermissionGate::DemoteUncovered
            && e.permission_coverage == PermissionCoverage::Uncovered
            && !e.pinned
        {
            meet.remove(&ExposureMode::Direct);
        }
        // `index_unverified` attestation gate (R2.8) — absent the gate a
        // lifted (unverified) entry may not serve `indexed`.
        if e.lifted && !gates.index_unverified {
            meet.remove(&ExposureMode::Indexed);
        }
        let source_key = e.source.as_str();
        let source_key = source_key.split(':').next().unwrap_or("harness");
        if let Some(default) = params.default_mode_by_source.get(source_key) {
            meet.retain(|m| m == default);
        }
        if meet.is_empty() {
            omitted.push(OmittedEntry {
                surface_id: e.surface_id.clone(),
                reason: OmitReason::ModeUnsupportedByProfile,
            });
            continue;
        }
        // Pinned ⇒ direct is mandatory (it must be in the meet or the
        // definition/profile deny it — I-NARROW: refusal, not coercion).
        if e.pinned || params.pinned_direct.contains(&e.surface_id) {
            if !meet.contains(&ExposureMode::Direct) {
                omitted.push(OmittedEntry {
                    surface_id: e.surface_id.clone(),
                    reason: OmitReason::ModeUnsupportedByProfile,
                });
                continue;
            }
            forced_direct.insert(e.surface_id.clone());
        }
        // Revealed surfaces enter this plan `direct` in append order (D6).
        if revealed_ids.contains(e.surface_id.as_str()) && meet.contains(&ExposureMode::Direct) {
            forced_direct.insert(e.surface_id.clone());
        }
        admitted.insert(e.surface_id.clone(), meet);
    }

    // policy.rank (or the kernel default).
    let mut choices: Vec<(String, ExposureMode)> = match policy {
        Some(p) => {
            let admissible: Vec<CatalogEntry> = catalog
                .entries
                .iter()
                .filter(|e| admitted.contains_key(&e.surface_id))
                .cloned()
                .collect();
            p.rank(&admissible, &admitted, turn_state, params)
        }
        None => direct_all(&catalog.entries, &admitted),
    };
    // Forced-direct members are invariant — a policy that omits them is a
    // PolicyViolation (I-NARROW); `direct_all` never omits them.
    for sid in &forced_direct {
        if !choices
            .iter()
            .any(|(s, m)| s == sid && *m == ExposureMode::Direct)
        {
            if policy.is_some() {
                return Err(SelectError::PolicyViolation {
                    detail: format!("pinned/revealed surface {sid} not returned direct"),
                });
            }
            choices.push((sid.clone(), ExposureMode::Direct));
        }
    }

    // Kernel enforce — I-NARROW + I-ONE-MODE.
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut entries_out: Vec<(String, ExposureMode)> = Vec::new();
    for (sid, mode) in choices {
        let Some(meet) = admitted.get(&sid) else {
            // Named a hidden/unavailable/unsupported entry — refused before
            // any delivery.
            let entry = catalog.entries.iter().find(|e| e.surface_id == sid);
            return Err(SelectError::PolicyViolation {
                detail: match entry {
                    Some(e) if e.hidden => format!("policy named hidden surface {sid}"),
                    Some(e) if e.availability != Availability::Available => {
                        format!("policy named unavailable surface {sid}")
                    }
                    _ => format!("policy named unadmitted surface {sid}"),
                },
            });
        };
        if !meet.contains(&mode) {
            return Err(SelectError::PolicyViolation {
                detail: format!("mode {} outside admitted_modes for {sid}", mode.as_str()),
            });
        }
        if !seen.insert(Box::leak(sid.clone().into_boxed_str())) {
            return Err(SelectError::PolicyViolation {
                detail: format!("surface {sid} assigned two modes"),
            });
        }
        entries_out.push((sid, mode));
    }

    // I-BUDGET: Σ tokens(direct) ≤ direct_token_share × window_cap. Pinned
    // overage ⇒ ExposureBudgetExceeded; others demote to omitted(Budget) in
    // reverse-selection order.
    let cap = (params.direct_token_share * turn_state.window_cap as f64) as u64;
    let tokens = |sid: &str| -> u64 {
        catalog
            .entries
            .iter()
            .find(|e| e.surface_id == sid)
            .map(|e| e.size_tokens)
            .unwrap_or(0)
    };
    let mut direct_tokens: u64 = entries_out
        .iter()
        .filter(|(_, m)| *m == ExposureMode::Direct)
        .map(|(s, _)| tokens(s))
        .sum();
    // Pinned overage is a refusal, never a demotion.
    let pinned_tokens: u64 = forced_direct.iter().map(|s| tokens(s)).sum();
    if pinned_tokens > cap {
        return Err(SelectError::ExposureBudgetExceeded {
            required: pinned_tokens,
            cap,
        });
    }
    if direct_tokens > cap {
        // Demote non-forced direct entries (last-selected first) to budget.
        let mut idx = entries_out.len();
        while direct_tokens > cap && idx > 0 {
            idx -= 1;
            let (sid, mode) = entries_out[idx].clone();
            if mode != ExposureMode::Direct || forced_direct.contains(&sid) {
                continue;
            }
            direct_tokens -= tokens(&sid);
            entries_out.remove(idx);
            omitted.push(OmittedEntry {
                surface_id: sid,
                reason: OmitReason::Budget,
            });
        }
    }
    if let Some(max) = params.max_direct_count {
        let mut count = entries_out
            .iter()
            .filter(|(_, m)| *m == ExposureMode::Direct)
            .count() as u64;
        let mut idx = entries_out.len();
        while count > max && idx > 0 {
            idx -= 1;
            let (sid, mode) = entries_out[idx].clone();
            if mode != ExposureMode::Direct || forced_direct.contains(&sid) {
                continue;
            }
            count -= 1;
            entries_out.remove(idx);
            omitted.push(OmittedEntry {
                surface_id: sid,
                reason: OmitReason::Budget,
            });
        }
    }

    // I-DISCOVERY: a `deferred` entry, or an `indexed` entry at granularity ≠
    // `surface`, requires a `direct` discovery capability.
    let needs_discovery = entries_out.iter().any(|(_, m)| {
        *m == ExposureMode::Deferred
            || (*m == ExposureMode::Indexed
                && params.index_granularity != IndexGranularity::Surface)
    });
    let discovery_ids: Vec<String> = entries_out
        .iter()
        .filter(|(sid, m)| {
            *m == ExposureMode::Direct
                && catalog
                    .entries
                    .iter()
                    .any(|e| e.surface_id == *sid && e.is_discovery)
        })
        .map(|(s, _)| s.clone())
        .collect();
    if needs_discovery && discovery_ids.is_empty() {
        return Err(SelectError::NoDiscoverySurface);
    }

    // I-ORDER: keep the prior plan's order for still-direct members, then
    // append new directs in catalog order; revealed members append in reveal
    // order (they are already in `forced_direct`/entries_out as direct).
    let direct_ids: Vec<String> = entries_out
        .iter()
        .filter(|(_, m)| *m == ExposureMode::Direct)
        .map(|(s, _)| s.clone())
        .collect();
    let mut order: Vec<String> = turn_state
        .prior_order
        .iter()
        .filter(|s| direct_ids.contains(s))
        .cloned()
        .collect();
    for sid in &direct_ids {
        if !order.contains(sid) {
            order.push(sid.clone());
        }
    }

    let mut plan = ExposurePlan {
        plan_id: String::new(),
        model_call_id: turn_state.model_call_id.clone(),
        catalog_id: catalog.catalog_id.clone(),
        entries: entries_out,
        discovery_surface_ids: discovery_ids,
        order,
        budget_estimate: BudgetEstimate {
            tokens: direct_tokens,
            estimator_ref: "bytes_div_4".to_string(),
        },
        omitted,
        derived_from: format!("{}:{}", catalog.catalog_id, turn_state.model_call_id),
        deterministic: true,
    };
    plan.plan_id = plan_id_of(&plan);
    Ok(plan)
}

/// `plan_id = H(canonical plan sans plan_id)`.
pub fn plan_id_of(p: &ExposurePlan) -> String {
    let body = Json::obj([
        ("catalog_id", Json::str(p.catalog_id.clone())),
        ("model_call_id", Json::str(p.model_call_id.clone())),
        (
            "entries",
            Json::Arr(
                p.entries
                    .iter()
                    .map(|(s, m)| Json::Arr(vec![Json::str(s.clone()), Json::str(m.as_str())]))
                    .collect(),
            ),
        ),
        (
            "order",
            Json::Arr(p.order.iter().map(|s| Json::str(s.clone())).collect()),
        ),
    ]);
    hh_identity::idp::idp_id("exposure_plan", body.to_canonical_string().as_bytes())
}

// ── check_callable / reveal / evict (ADR-0093 D6/D7) ─────────────────────────

/// A proposal for `check_callable` — the surface name the model emitted
/// (`surface_name` verbatim — T4).
#[derive(Debug, Clone, PartialEq)]
pub struct CallProposal {
    /// The emitted surface name.
    pub surface_name: String,
    /// `true` when the proposal originated inside an executed program
    /// (code-mode calls check `code_mode`, not `direct` — C2).
    pub from_code: bool,
}

/// `check_callable(plan_of(model_call_id), catalog, proposal)` — `ok` iff the
/// named surface was `direct` in the producing call's plan (or `code_mode`
/// for program-originated proposals — C2). `SurfaceNotRevealed` never carries
/// a definition; `SurfaceUnavailable` names the availability reason.
/// `check_callable` precedes the monitor and never replaces it (I-NOAUTH).
pub fn check_callable(
    plan: &ExposurePlan,
    catalog: &Catalog,
    proposal: &CallProposal,
) -> Result<(), CallRefusal> {
    let entry = catalog
        .entries
        .iter()
        .find(|e| e.name == proposal.surface_name);
    // An unknown name: `SurfaceNotRevealed{surface_id: None, hint: none}` —
    // the refusal carries no schema or description (AC-R-2.5.3-1).
    let Some(entry) = entry else {
        return Err(CallRefusal::SurfaceNotRevealed {
            surface_id: None,
            hint: RevealHint::None,
        });
    };
    if let Availability::Unavailable(reason) = &entry.availability {
        return Err(CallRefusal::SurfaceUnavailable {
            reason: reason.clone(),
        });
    }
    if let Availability::Stale(epoch) = entry.availability {
        return Err(CallRefusal::SurfaceUnavailable {
            reason: format!("stale epoch {epoch}"),
        });
    }
    let plan_mode = plan
        .entries
        .iter()
        .find(|(sid, _)| *sid == entry.surface_id)
        .map(|(_, m)| *m);
    let callable = match (proposal.from_code, plan_mode) {
        (true, Some(ExposureMode::CodeMode)) => true,
        (true, _) => false,
        (false, Some(ExposureMode::Direct)) => true,
        (false, _) => false,
    };
    if callable {
        return Ok(());
    }
    let hint = if entry
        .admitted_modes
        .iter()
        .any(|m| matches!(m, ExposureMode::Indexed | ExposureMode::Deferred))
    {
        RevealHint::Search
    } else {
        RevealHint::None
    };
    Err(CallRefusal::SurfaceNotRevealed {
        surface_id: Some(entry.surface_id.clone()),
        hint,
    })
}

/// `reveal(revealed, surface_ids, cause, retention, at_seq)` — revealed
/// entries enter the *next* plan `direct` in append order; every surface id
/// must be a catalog member of the current epoch (I-CLOSED/I-EPOCH →
/// `NotInCatalog` — `IndexStale` is the same refusal).
pub fn reveal(
    revealed: &RevealedSet,
    catalog: &Catalog,
    surface_ids: &[String],
    cause: RevealCause,
    retention: Retention,
    at_seq: u64,
) -> Result<RevealedSet, RevealError> {
    let mut out = revealed.clone();
    for sid in surface_ids {
        if !catalog.entries.iter().any(|e| e.surface_id == *sid) {
            return Err(RevealError::NotInCatalog {
                surface_id: sid.clone(),
            });
        }
        if out.entries.iter().any(|e| e.surface_id == *sid) {
            continue; // already revealed — idempotent
        }
        out.entries.push(RevealedEntry {
            surface_id: sid.clone(),
            mode: ExposureMode::Direct,
            revealed_at: at_seq,
            cause,
            retention,
        });
    }
    Ok(out)
}

/// `evict(revealed, catalog, surface_ids, cause)` — pinned surfaces cannot be
/// evicted (silently kept — the `action.tool.surface.evicted` row is the
/// caller's; the refusal surface for evicting a non-member is `NotInCatalog`).
pub fn evict(
    revealed: &RevealedSet,
    catalog: &Catalog,
    surface_ids: &[String],
    _cause: EvictCause,
) -> RevealedSet {
    let mut out = revealed.clone();
    out.entries.retain(|e| {
        if !surface_ids.contains(&e.surface_id) {
            return true;
        }
        // Pinned surfaces cannot be evicted.
        catalog
            .entries
            .iter()
            .any(|c| c.surface_id == e.surface_id && c.pinned)
    });
    out
}

/// A reveal-retention boundary (§5d.3 §3 `retention`; ADR-0093 D6 — the
/// S2.10 "RevealedSet retention" slice).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevealBoundary {
    /// The producing model call ended — `retention = call` entries expire.
    CallEnd,
    /// The producing turn ended — `call` and `turn` entries expire.
    TurnEnd,
    /// The run ended — every retention class expires (the run's revealed
    /// set is run state; this is the terminal fold, used by projections,
    /// not a delivery change).
    RunEnd,
}

/// `expire_reveals(revealed, catalog, boundary)` — drop the entries whose
/// `retention` ends at `boundary`; `run`/`until_evicted` survive call/turn
/// boundaries (`RunEnd` ends all). Pinned surfaces never expire (the same
/// kernel rule `evict` enforces — a pinned surface stays `direct`).
///
/// Returns `(revealed', expired[])` — `expired` is the surface-id list the
/// caller emits as `action.tool.surface.evicted{cause: policy}` rows (the
/// event is the caller's; this is the pure fold).
pub fn expire_reveals(
    revealed: &RevealedSet,
    catalog: &Catalog,
    boundary: RevealBoundary,
) -> (RevealedSet, Vec<String>) {
    let mut out = revealed.clone();
    let mut expired = Vec::new();
    out.entries.retain(|e| {
        // Pinned surfaces never expire.
        let pinned = catalog
            .entries
            .iter()
            .any(|c| c.surface_id == e.surface_id && c.pinned);
        if pinned {
            return true;
        }
        let keep = !matches!(
            (boundary, e.retention),
            (RevealBoundary::CallEnd, Retention::Call)
                | (RevealBoundary::TurnEnd, Retention::Call | Retention::Turn)
                | (RevealBoundary::RunEnd, _)
        );
        if !keep {
            expired.push(e.surface_id.clone());
        }
        keep
    });
    (out, expired)
}

// ── Entry codec + catalog payloads (R2.8 — the durable-fold halves) ────────

/// `catalog_entry_json` — the canonical entry record (the durable
/// `catalog.built`/`catalog.delta` member and the resume fold's decode
/// input; CC1 — one spelling, both directions live here).
pub fn catalog_entry_json(e: &CatalogEntry) -> Json {
    let mode_json = |m: &ExposureMode| Json::str(m.as_str());
    Json::obj([
        ("surface_id", Json::str(e.surface_id.clone())),
        ("capability", Json::str(e.capability.clone())),
        ("version_id", Json::str(e.version_id.clone())),
        ("name", Json::str(e.name.clone())),
        (
            "namespace",
            e.namespace.clone().map_or(Json::Null, Json::str),
        ),
        ("source", Json::str(e.source.as_str())),
        (
            "admitted_modes",
            Json::Arr(e.admitted_modes.iter().map(mode_json).collect()),
        ),
        ("pinned", Json::Bool(e.pinned)),
        ("hidden", Json::Bool(e.hidden)),
        ("lifted", Json::Bool(e.lifted)),
        ("is_discovery", Json::Bool(e.is_discovery)),
        (
            "index_form",
            Json::obj([
                ("name", Json::str(e.index_form.name.clone())),
                (
                    "title",
                    e.index_form.title.clone().map_or(Json::Null, Json::str),
                ),
                ("summary_ref", Json::str(e.index_form.summary_ref.clone())),
                (
                    "namespace",
                    e.index_form.namespace.clone().map_or(Json::Null, Json::str),
                ),
                (
                    "tags",
                    Json::Arr(e.index_form.tags.iter().map(Json::str).collect()),
                ),
                (
                    "search_text_fields",
                    Json::Arr(
                        e.index_form
                            .search_text_fields
                            .iter()
                            .map(|f| Json::str(f.as_str()))
                            .collect(),
                    ),
                ),
                (
                    "search_text",
                    Json::Obj(
                        e.index_form
                            .search_text
                            .iter()
                            .map(|(f, t)| (f.as_str().to_string(), Json::str(t.clone())))
                            .collect(),
                    ),
                ),
            ]),
        ),
        ("size_tokens", Json::Int(e.size_tokens as i64)),
        ("estimator_ref", Json::str(e.estimator_ref.clone())),
        (
            "effect_summary",
            Json::Arr(e.effect_summary.iter().map(Json::str).collect()),
        ),
        (
            "permission_coverage",
            Json::str(match e.permission_coverage {
                PermissionCoverage::Covered => "covered",
                PermissionCoverage::Ask => "ask",
                PermissionCoverage::Uncovered => "uncovered",
                PermissionCoverage::Unknown => "unknown",
            }),
        ),
        ("label", e.label.clone().map_or(Json::Null, Json::str)),
        (
            "availability",
            match &e.availability {
                Availability::Available => Json::str("available"),
                Availability::Unavailable(r) => Json::obj([
                    ("kind", Json::str("unavailable")),
                    ("reason", Json::str(r.clone())),
                ]),
                Availability::Stale(epoch) => Json::obj([
                    ("kind", Json::str("stale")),
                    ("epoch", Json::Int(*epoch as i64)),
                ]),
            },
        ),
    ])
}

/// `catalog_entry_from_json` — the read half of [`catalog_entry_json`]
/// (`None` on a malformed member — the fold's caller names the skip, the
/// codec never invents a member).
pub fn catalog_entry_from_json(j: &Json) -> Option<CatalogEntry> {
    let s = |k: &str| j.get(k).and_then(Json::as_str).map(str::to_string);
    let b = |k: &str| matches!(j.get(k), Some(Json::Bool(true)));
    let form_j = j.get("index_form")?;
    let mut fields = BTreeSet::new();
    if let Some(Json::Arr(fs)) = form_j.get("search_text_fields") {
        for f in fs {
            fields.insert(SearchTextField::parse(f.as_str()?)?);
        }
    }
    let mut search_text = BTreeMap::new();
    if let Some(Json::Obj(m)) = form_j.get("search_text") {
        for (k, v) in m {
            search_text.insert(SearchTextField::parse(k)?, v.as_str()?.to_string());
        }
    }
    Some(CatalogEntry {
        surface_id: s("surface_id")?,
        capability: s("capability")?,
        version_id: s("version_id")?,
        source: CatalogSource::parse(&s("source")?)?,
        name: s("name")?,
        namespace: s("namespace"),
        admitted_modes: match j.get("admitted_modes") {
            Some(Json::Arr(ms)) => {
                let mut set = BTreeSet::new();
                for m in ms {
                    set.insert(ExposureMode::parse(m.as_str()?)?);
                }
                set
            }
            _ => BTreeSet::new(),
        },
        pinned: b("pinned"),
        hidden: b("hidden"),
        lifted: b("lifted"),
        is_discovery: b("is_discovery"),
        index_form: IndexForm {
            name: form_j.get("name")?.as_str()?.to_string(),
            title: form_j
                .get("title")
                .and_then(Json::as_str)
                .map(str::to_string),
            summary_ref: form_j
                .get("summary_ref")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string(),
            namespace: form_j
                .get("namespace")
                .and_then(Json::as_str)
                .map(str::to_string),
            tags: match form_j.get("tags") {
                Some(Json::Arr(ts)) => ts
                    .iter()
                    .filter_map(|t| t.as_str().map(str::to_string))
                    .collect(),
                _ => Vec::new(),
            },
            search_text_fields: fields,
            search_text,
        },
        size_tokens: j.get("size_tokens")?.as_int()?.max(0) as u64,
        estimator_ref: s("estimator_ref").unwrap_or_default(),
        effect_summary: match j.get("effect_summary") {
            Some(Json::Arr(es)) => es
                .iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        },
        permission_coverage: match s("permission_coverage")?.as_str() {
            "covered" => PermissionCoverage::Covered,
            "ask" => PermissionCoverage::Ask,
            "uncovered" => PermissionCoverage::Uncovered,
            "unknown" => PermissionCoverage::Unknown,
            _ => return None,
        },
        label: s("label"),
        availability: match j.get("availability") {
            Some(Json::Str(a)) if a == "available" => Availability::Available,
            Some(o) => match o.get("kind").and_then(Json::as_str) {
                Some("unavailable") => {
                    Availability::Unavailable(o.get("reason")?.as_str()?.to_string())
                }
                Some("stale") => Availability::Stale(o.get("epoch")?.as_int()?.max(0) as u64),
                _ => return None,
            },
            _ => return None,
        },
    })
}

/// `source_state_json` — the `catalog.built` `sources[]` member spelling.
pub fn source_state_json(s: &SourceState) -> Json {
    Json::obj([
        ("source_ref", Json::str(s.source_ref.clone())),
        ("snapshot_hash", Json::str(s.snapshot_hash.clone())),
        ("ttl", s.ttl.map_or(Json::Null, |t| Json::Int(t as i64))),
        ("listened", Json::Bool(s.listened)),
    ])
}

/// `action.tool.catalog.built` — the catalog record the driver appends when
/// an exposure-aware run arms (R2.8): `{catalog_id, epoch, bundle_id,
/// sources[], entries[], index_ref}` — the entries are the *full* entry
/// records (the durable fold rebuilds the shadow catalog from this row —
/// the projection rows (`name`/`namespace`/`lifted`) are inside the entry
/// records).
pub fn catalog_built_payload(catalog: &Catalog) -> Json {
    Json::obj([
        ("catalog_id", Json::str(catalog.catalog_id.clone())),
        ("epoch", Json::Int(catalog.epoch as i64)),
        ("bundle_id", Json::str(catalog.bundle_id.clone())),
        (
            "index_ref",
            catalog.index_ref.clone().map_or(Json::Null, Json::str),
        ),
        (
            "indexes",
            Json::Arr(catalog.indexes.iter().map(index_decl_json).collect()),
        ),
        (
            "sources",
            Json::Arr(catalog.sources.iter().map(source_state_json).collect()),
        ),
        (
            "entries",
            Json::Arr(catalog.entries.iter().map(catalog_entry_json).collect()),
        ),
    ])
}

/// `catalog_built_from_payload` — the resume-fold read of
/// [`catalog_built_payload`] (`None` on a malformed row — the caller folds
/// only well-formed rows; a malformed durable row is a stop, never a
/// guess).
pub fn catalog_built_from_payload(j: &Json) -> Option<Catalog> {
    let mut entries = Vec::new();
    if let Some(Json::Arr(es)) = j.get("entries") {
        for e in es {
            entries.push(catalog_entry_from_json(e)?);
        }
    }
    let mut sources = Vec::new();
    if let Some(Json::Arr(ss)) = j.get("sources") {
        for s in ss {
            let g = |k: &str| s.get(k).and_then(Json::as_str).map(str::to_string);
            sources.push(SourceState {
                source_ref: g("source_ref")?,
                snapshot_hash: g("snapshot_hash")?,
                ttl: s.get("ttl").and_then(Json::as_int).map(|t| t.max(0) as u64),
                listened: matches!(s.get("listened"), Some(Json::Bool(true))),
            });
        }
    }
    let mut indexes = Vec::new();
    if let Some(Json::Arr(ds)) = j.get("indexes") {
        for d in ds {
            indexes.push(IndexDecl {
                index: index_from_json(d.get("index")?)?,
                granularity: IndexGranularity::parse(d.get("granularity")?.as_str()?)?,
            });
        }
    }
    Some(Catalog {
        catalog_id: j.get("catalog_id")?.as_str()?.to_string(),
        epoch: j.get("epoch")?.as_int()?.max(0) as u64,
        bundle_id: j.get("bundle_id")?.as_str()?.to_string(),
        sources,
        entries,
        indexes,
        index_ref: j
            .get("index_ref")
            .and_then(Json::as_str)
            .map(str::to_string),
    })
}

/// `index_from_json` — the read half of [`index_json`].
pub fn index_from_json(j: &Json) -> Option<CatalogIndex> {
    match j.get("kind")?.as_str()? {
        "exact_name" => Some(CatalogIndex::ExactName),
        "static_allowlist" => {
            let mut ids = BTreeSet::new();
            if let Some(Json::Arr(xs)) = j.get("surface_ids") {
                for x in xs {
                    ids.insert(x.as_str()?.to_string());
                }
            }
            Some(CatalogIndex::StaticAllowlist(ids))
        }
        "lexical_regex" => Some(CatalogIndex::LexicalRegex),
        "bm25" => Some(CatalogIndex::Bm25),
        "hierarchical" => Some(CatalogIndex::Hierarchical),
        "embedding" => Some(CatalogIndex::Embedding {
            similarity_threshold_ppm: j.get("similarity_threshold_ppm")?.as_int()?.max(0) as u64,
        }),
        "model_ranked" => Some(CatalogIndex::ModelRanked),
        "filesystem" => Some(CatalogIndex::Filesystem),
        _ => None,
    }
}

/// `action.tool.catalog.epoch` payload — the `CatalogEpoch`'s durable
/// spelling (extracted so `sync_source` and the driver share the row —
/// CC1).
pub fn catalog_epoch_payload(epoch: &CatalogEpoch) -> Json {
    Json::obj([
        ("epoch", Json::Int(epoch.epoch as i64)),
        ("catalog_id_prev", Json::str(epoch.catalog_id_prev.clone())),
        ("catalog_id", Json::str(epoch.catalog_id.clone())),
        ("bundle_delta", epoch.bundle_delta.clone()),
        (
            "loss_report",
            epoch.loss_report.clone().unwrap_or(Json::Null),
        ),
        ("adopted_by", Json::str(epoch.adopted_by.clone())),
    ])
}

// ── Runtime event payloads (R2.8 — the §5d.3 §7 emitter half) ────────────────

/// `action.tool.exposure.planned{model_call_id, plan_id, catalog_id,
/// mode_changes[], omitted[], budget_estimate}` (§5d.3 §7; scope
/// `context_assembly` — the driver's call scope carries it). `mode_changes[]`
/// diffs `plan` against the prior call's `prev` — `{surface_id, mode_from,
/// mode_to}` with the closed `ExposureMode` spellings; `none` names an
/// absent leg (a surface that entered or left the plan).
pub fn exposure_planned_payload(plan: &ExposurePlan, prev: Option<&ExposurePlan>) -> Json {
    let prev_modes: BTreeMap<&str, ExposureMode> = prev
        .map(|p| p.entries.iter().map(|(s, m)| (s.as_str(), *m)).collect())
        .unwrap_or_default();
    let cur_modes: BTreeMap<&str, ExposureMode> =
        plan.entries.iter().map(|(s, m)| (s.as_str(), *m)).collect();
    let mut mode_changes = Vec::new();
    for (sid, mode) in &cur_modes {
        let from = prev_modes.get(sid).copied();
        if from != Some(*mode) {
            mode_changes.push(Json::obj([
                ("surface_id", Json::str(*sid)),
                (
                    "mode_from",
                    from.map_or_else(|| Json::str("none"), |m| Json::str(m.as_str())),
                ),
                ("mode_to", Json::str(mode.as_str())),
            ]));
        }
    }
    for (sid, mode) in &prev_modes {
        if !cur_modes.contains_key(sid) {
            mode_changes.push(Json::obj([
                ("surface_id", Json::str(*sid)),
                ("mode_from", Json::str(mode.as_str())),
                ("mode_to", Json::str("none")),
            ]));
        }
    }
    Json::obj([
        ("model_call_id", Json::str(plan.model_call_id.clone())),
        ("plan_id", Json::str(plan.plan_id.clone())),
        ("catalog_id", Json::str(plan.catalog_id.clone())),
        (
            "budget_estimate",
            Json::obj([
                ("tokens", Json::Int(plan.budget_estimate.tokens as i64)),
                (
                    "estimator_ref",
                    Json::str(plan.budget_estimate.estimator_ref.clone()),
                ),
            ]),
        ),
        ("mode_changes", Json::Arr(mode_changes)),
        // `order` rides beside `mode_changes` — the resume fold's
        // I-ORDER prefix input (the payload's audit members are the
        // spec's; `order` is the fold's own record).
        (
            "order",
            Json::Arr(plan.order.iter().map(|s| Json::str(s.clone())).collect()),
        ),
        (
            "omitted",
            Json::Arr(
                plan.omitted
                    .iter()
                    .map(|o| {
                        Json::obj([
                            ("surface_id", Json::str(o.surface_id.clone())),
                            ("reason", Json::str(o.reason.as_str())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

/// `action.tool.discovery.searched{tool_call_id, query_form, query_hash,
/// executed_by, variant_ref, hits[], truncated, cost}` (§5d.3 §7).
/// `query_hash` is the idp/1 content address of the canonical
/// `(form_kind, granularity, text)` triple — the query text itself never
/// lands on the row (the hash is the join; ADR-0094 D6).
pub fn discovery_searched_payload(
    tool_call_id: &str,
    query: &DiscoveryQuery,
    index: &CatalogIndex,
    result: &DiscoveryResult,
) -> Json {
    let query_hash = hh_identity::idp::idp_id(
        "discovery_query",
        format!(
            "{}\n{}\n{}",
            DiscoveryFormKind::of(&query.form).as_str(),
            query.granularity.as_str(),
            query.text,
        )
        .as_bytes(),
    );
    Json::obj([
        ("tool_call_id", Json::str(tool_call_id)),
        (
            "query_form",
            Json::str(DiscoveryFormKind::of(&query.form).as_str()),
        ),
        ("query_hash", Json::str(query_hash)),
        ("executed_by", Json::str(result.executed_by.clone())),
        ("variant_ref", Json::str(index_ref(index))),
        (
            "hits",
            Json::Arr(
                result
                    .hits
                    .iter()
                    .map(|h| {
                        let mut m = vec![
                            ("surface_id", Json::str(h.surface_id.clone())),
                            ("rank", Json::Int(h.rank as i64)),
                        ];
                        if let Some(s) = &h.score {
                            m.push(("score", s.clone()));
                        }
                        Json::obj(m)
                    })
                    .collect(),
            ),
        ),
        ("truncated", Json::Bool(result.truncated)),
        ("cost", result.cost.clone().unwrap_or(Json::Null)),
    ])
}

/// `action.tool.surface.revealed{surface_id, mode_from, mode_to, cause,
/// retention}` (§5d.3 §7) — `mode_from` is the catalog mode the surface
/// held before the reveal (`deferred`/`indexed`), `mode_to` is the mode it
/// enters (`direct` — reveals land `direct` in the next plan).
pub fn surface_revealed_payload(entry: &RevealedEntry, mode_from: ExposureMode) -> Json {
    Json::obj([
        ("surface_id", Json::str(entry.surface_id.clone())),
        ("mode_from", Json::str(mode_from.as_str())),
        ("mode_to", Json::str(entry.mode.as_str())),
        ("cause", Json::str(entry.cause.as_str())),
        ("retention", Json::str(entry.retention.as_str())),
    ])
}

/// `action.tool.surface.evicted{surface_id, cause}` (§5d.3 §7).
pub fn surface_evicted_payload(surface_id: &str, cause: EvictCause) -> Json {
    Json::obj([
        ("surface_id", Json::str(surface_id)),
        ("cause", Json::str(cause.as_str())),
    ])
}

/// `action.tool.call.refused{surface_id?, reason, hint}` (§5d.3 §7;
/// ADR-0093 D7) — the refusal carries no definition: `surface_id` lands
/// only when the catalog resolved the attempted name; `hint` names the
/// `search`/`none` leg the model could take.
pub fn call_refused_payload(refusal: &CallRefusal) -> Json {
    match refusal {
        CallRefusal::SurfaceNotRevealed { surface_id, hint } => Json::obj([
            (
                "surface_id",
                surface_id.clone().map_or(Json::Null, Json::str),
            ),
            ("reason", Json::str("surface_not_revealed")),
            (
                "hint",
                Json::str(match hint {
                    RevealHint::Search => "search",
                    RevealHint::None => "none",
                }),
            ),
        ]),
        CallRefusal::SurfaceUnavailable { reason } => Json::obj([
            ("surface_id", Json::Null),
            ("reason", Json::str("surface_unavailable")),
            ("detail", Json::str(reason.clone())),
            ("hint", Json::str("none")),
        ]),
    }
}

/// `discovery_query_from_args(args)` — the kernel `discover_surfaces`
/// capability's argument grammar (R2.8): `{"query": string}` is required;
/// `form ∈ {regex, natural_language, structured}` selects the form kind
/// (default `natural_language`); `limit` bounds hits; `granularity` is the
/// `IndexGranularity` spelling (default `surface`); the `structured` form
/// reads `namespace`/`effect_filter`/`tags[]`/`source` off the same
/// object. A malformed member is a `DiscoveryQueryInvalid` — the typed
/// refusal, never a defaulted guess.
pub fn discovery_query_from_args(args: &Json) -> Result<DiscoveryQuery, DiscoveryQueryInvalid> {
    let text = match args.get("query").or_else(|| args.get("text")) {
        Some(Json::Str(s)) => s.clone(),
        _ => return Err(DiscoveryQueryInvalid::Empty),
    };
    if text.is_empty() {
        return Err(DiscoveryQueryInvalid::Empty);
    }
    if text.chars().count() > 4096 {
        return Err(DiscoveryQueryInvalid::TooLong);
    }
    let form = match args.get("form").and_then(Json::as_str) {
        None | Some("natural_language") | Some("nl") => DiscoveryForm::NaturalLanguage,
        Some("regex") => DiscoveryForm::Regex,
        Some("structured") => DiscoveryForm::Structured(StructuredQuery {
            namespace: args
                .get("namespace")
                .and_then(Json::as_str)
                .map(str::to_string),
            effect_filter: args
                .get("effect_filter")
                .and_then(Json::as_str)
                .map(str::to_string),
            tags: match args.get("tags") {
                Some(Json::Arr(ts)) => ts
                    .iter()
                    .filter_map(|t| t.as_str().map(str::to_string))
                    .collect(),
                _ => Vec::new(),
            },
            source: args
                .get("source")
                .and_then(Json::as_str)
                .map(str::to_string),
        }),
        Some(_) => return Err(DiscoveryQueryInvalid::UnsupportedForm),
    };
    let granularity = match args.get("granularity").and_then(Json::as_str) {
        None => IndexGranularity::Surface,
        Some(g) => IndexGranularity::parse(g).ok_or(DiscoveryQueryInvalid::UnsupportedForm)?,
    };
    Ok(DiscoveryQuery {
        form,
        text,
        limit: args
            .get("limit")
            .and_then(Json::as_int)
            .map(|i| i.max(0) as u64),
        granularity,
    })
}
