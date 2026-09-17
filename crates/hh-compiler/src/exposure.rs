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
}

/// `IndexGranularity ∈ {surface, namespace, source}` — the granularity an
/// `indexed` entry is delivered at (`surface` is the C0 granularity; the
/// others need a `direct` discovery capability — I-DISCOVERY).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IndexGranularity {
    /// One index form per surface.
    Surface,
    /// One index form per namespace.
    Namespace,
    /// One index form per source.
    Source,
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
    /// The content-addressed index ref, when a `catalog_index` is bound.
    pub index_ref: Option<String>,
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

/// `DiscoveryQuery{form, text, limit?}` — the `discover_surfaces` input.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveryQuery {
    /// The form.
    pub form: DiscoveryForm,
    /// The query text (`""` is `DiscoveryQueryInvalid::Empty`).
    pub text: String,
    /// The caller's hit bound.
    pub limit: Option<u64>,
}

/// `DiscoveryQueryInvalid{too_long | empty | unsupported_form}`.
#[derive(Debug, Clone, PartialEq)]
pub enum DiscoveryQueryInvalid {
    /// The text exceeded the bound (4096 chars).
    TooLong,
    /// The text was empty.
    Empty,
    /// The index variant cannot serve the form (C1+ indexes declare forms).
    UnsupportedForm,
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

/// The C0 `catalog_index` variants (ADR-0094 D4): `exact_name` and
/// `static_allowlist`. `hidden` entries are never indexed (I-CLOSED ∩
/// AC-R-2.5.3-12).
#[derive(Debug, Clone, PartialEq)]
pub enum CatalogIndex {
    /// `exact_name` — the query text matches `IndexForm.name` literally.
    ExactName,
    /// `static_allowlist` — the declared surface-id set, filtered by the
    /// query's structured members.
    StaticAllowlist(BTreeSet<String>),
}

/// The content-addressed index ref (`charged instrument` — the ref is
/// `H(canonical index form)`).
pub fn index_ref(index: &CatalogIndex) -> String {
    let body = match index {
        CatalogIndex::ExactName => Json::obj([("kind", Json::str("exact_name"))]),
        CatalogIndex::StaticAllowlist(ids) => Json::obj([
            ("kind", Json::str("static_allowlist")),
            (
                "surface_ids",
                Json::Arr(ids.iter().map(|i| Json::str(i.clone())).collect()),
            ),
        ]),
    };
    hh_identity::idp::idp_id("catalog_index", body.to_canonical_string().as_bytes())
}

/// `query(index, catalog, query, max_reveal)` — `hits ⊆ entries` with mode ∈
/// `{indexed, deferred}` semantics is the plan's job; the index serves
/// **name-level** hits over non-hidden entries (I-CLOSED). `|hits| ≤
/// min(query.limit, max_reveal_per_search)`; an empty result is normal.
pub fn index_query(
    index: &CatalogIndex,
    catalog: &Catalog,
    query: &DiscoveryQuery,
    max_reveal_per_search: u64,
) -> Result<DiscoveryResult, DiscoveryQueryInvalid> {
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
    let structured = match &query.form {
        DiscoveryForm::Structured(s) => Some(s.clone()),
        _ => None,
    };
    let matches_filter = |e: &CatalogEntry| -> bool {
        if let Some(s) = &structured {
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
        }
        true
    };
    let mut hits: Vec<&CatalogEntry> = match index {
        CatalogIndex::ExactName => catalog
            .entries
            .iter()
            .filter(|e| !e.hidden && e.name == query.text && matches_filter(e))
            .collect(),
        CatalogIndex::StaticAllowlist(ids) => catalog
            .entries
            .iter()
            .filter(|e| {
                !e.hidden
                    && ids.contains(&e.surface_id)
                    && matches_filter(e)
                    && (structured.is_some() || e.name == query.text)
            })
            .collect(),
    };
    hits.sort_by(|a, b| a.surface_id.cmp(&b.surface_id));
    let truncated = hits.len() as u64 > bound;
    hits.truncate(bound as usize);
    Ok(DiscoveryResult {
        hits: hits
            .iter()
            .enumerate()
            .map(|(i, e)| DiscoveryHit {
                surface_id: e.surface_id.clone(),
                rank: i as u64 + 1,
                score: None,
            })
            .collect(),
        truncated,
        executed_by: "kernel".to_string(),
        cost: None,
    })
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

/// `CatalogDelta{source_ref, cause, added[], removed[], changed[]}` — every
/// delta is emitted as `action.tool.catalog.delta`, including empty ones.
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

// ── Errors (the spec's error column) ─────────────────────────────────────────

/// `CatalogBuildError` — `source_unavailable | IndexOverflow{pages | bytes}`.
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
    let mut entries = Vec::new();
    for tool in &bundle.runtime_plan.tools {
        let Some(binding) = &tool.surface else {
            continue;
        };
        entries.push(entry_from_binding(bundle, tool, binding, sources_state));
    }
    entries.sort_by(|a, b| a.surface_id.cmp(&b.surface_id));
    let catalog_id = catalog_id_of(&entries, epoch, &bundle.bundle_id);
    Ok(Catalog {
        catalog_id,
        epoch,
        bundle_id: bundle.bundle_id.clone(),
        sources: source_states,
        entries,
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
        },
        size_tokens: surface_size_tokens(binding),
        estimator_ref: "bytes_div_4".to_string(),
        effect_summary: binding.effects_bound.clone(),
        permission_coverage: PermissionCoverage::Unknown,
        label: None,
        availability: Availability::Available,
        is_discovery: tool_is_discovery(tool),
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
        // I-NARROW: definition ∩ profile ∩ policy. `default_mode_by_source`
        // narrows to its singleton when it names this source; `permission_gate`
        // demotes `uncovered` (OQ-234 default) but never hides at C0.
        let mut meet: BTreeSet<ExposureMode> = e
            .admitted_modes
            .intersection(profile_modes)
            .copied()
            .collect();
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
