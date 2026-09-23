//! `ModelProfile/1` (§3.2.3): the closed `ProfileRule` kinds, the debt-record schema
//! (§3.2.8 — "assumption debt", the contract's term), the selector/chain model, and the
//! `CapabilityDeclaration`. `ModelProfile` is sealed-`ext`: every extension block is
//! typed, metered, and carries its own debt record.

use std::collections::BTreeMap;

use hh_wire::json::Json;

use crate::errors::CompileError;

/// The closed rule-kind set — the parts of the model surface a `ModelProfile/1` is
/// permitted to write (spec §3.2.3: `kind ∈ {tool_shape, naming, schema_dialect,
/// description_template, error_format, result_render, prompt_layout,
/// interaction_mode, transcript_render, compaction_reminder, sampling_defaults,
/// caching_markers, procedure_target}` — a closed enum per profile-schema version;
/// `procedure_target`, owning `compile_hint`, is admitted at Phase 2 per ADR-0085 /
/// CF-316). A rule writes only surface records and profile parameters, never
/// `Permission`, `EffectClass`, `Budget`, `Validator` or identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProfileRuleKind {
    /// `tool_shape` — selects a surface family **by id** (`SurfaceFamilyRef{family_id,
    /// variant_id}`), never an inline surface (CF-199).
    ToolShape,
    /// `naming` — the tool-naming scheme (the minimal profiles differ prefix vs suffix).
    Naming,
    /// `schema_dialect`.
    SchemaDialect,
    /// `description_template`.
    DescriptionTemplate,
    /// `error_format`.
    ErrorFormat,
    /// `result_render`.
    ResultRender,
    /// `prompt_layout` — yields D1's `Layout` + `role_map` + demotion wrappers.
    PromptLayout,
    /// `interaction_mode` — C0 admits `native_fc` only.
    InteractionMode,
    /// `transcript_render`.
    TranscriptRender,
    /// `compaction_reminder` — a C0/Stage-2 schema slice of its params (CF-271).
    CompactionReminder,
    /// `sampling_defaults`.
    SamplingDefaults,
    /// `caching_markers`.
    CachingMarkers,
    /// `procedure_target` — owns `compile_hint`; admitted at Phase 2 (ADR-0085; CF-316).
    ProcedureTarget,
}

impl ProfileRuleKind {
    /// Canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            ProfileRuleKind::ToolShape => "tool_shape",
            ProfileRuleKind::Naming => "naming",
            ProfileRuleKind::SchemaDialect => "schema_dialect",
            ProfileRuleKind::DescriptionTemplate => "description_template",
            ProfileRuleKind::ErrorFormat => "error_format",
            ProfileRuleKind::ResultRender => "result_render",
            ProfileRuleKind::PromptLayout => "prompt_layout",
            ProfileRuleKind::InteractionMode => "interaction_mode",
            ProfileRuleKind::TranscriptRender => "transcript_render",
            ProfileRuleKind::CompactionReminder => "compaction_reminder",
            ProfileRuleKind::SamplingDefaults => "sampling_defaults",
            ProfileRuleKind::CachingMarkers => "caching_markers",
            ProfileRuleKind::ProcedureTarget => "procedure_target",
        }
    }

    /// Parse a spelling; `None` on any value outside the closed set.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "tool_shape" => ProfileRuleKind::ToolShape,
            "naming" => ProfileRuleKind::Naming,
            "schema_dialect" => ProfileRuleKind::SchemaDialect,
            "description_template" => ProfileRuleKind::DescriptionTemplate,
            "error_format" => ProfileRuleKind::ErrorFormat,
            "result_render" => ProfileRuleKind::ResultRender,
            "prompt_layout" => ProfileRuleKind::PromptLayout,
            "interaction_mode" => ProfileRuleKind::InteractionMode,
            "transcript_render" => ProfileRuleKind::TranscriptRender,
            "compaction_reminder" => ProfileRuleKind::CompactionReminder,
            "sampling_defaults" => ProfileRuleKind::SamplingDefaults,
            "caching_markers" => ProfileRuleKind::CachingMarkers,
            "procedure_target" => ProfileRuleKind::ProcedureTarget,
            _ => return None,
        })
    }

    /// Every member — the closed set, in spec order.
    pub const ALL: &'static [ProfileRuleKind] = &[
        ProfileRuleKind::ToolShape,
        ProfileRuleKind::Naming,
        ProfileRuleKind::SchemaDialect,
        ProfileRuleKind::DescriptionTemplate,
        ProfileRuleKind::ErrorFormat,
        ProfileRuleKind::ResultRender,
        ProfileRuleKind::PromptLayout,
        ProfileRuleKind::InteractionMode,
        ProfileRuleKind::TranscriptRender,
        ProfileRuleKind::CompactionReminder,
        ProfileRuleKind::SamplingDefaults,
        ProfileRuleKind::CachingMarkers,
        ProfileRuleKind::ProcedureTarget,
    ];
}

/// `compliance{detector_class, followed_predicate_ref?}` (§3.2.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compliance {
    /// `deterministic` — a machine check can verify the rule's target artefact; `judged` —
    /// the check is a judged predicate (requires `followed_predicate_ref`); `none` — no
    /// compliance check is claimed.
    pub detector_class: ComplianceDetector,
    /// The judged predicate (required iff `detector_class == Judged`).
    pub followed_predicate_ref: Option<String>,
}

/// The closed compliance-detector classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComplianceDetector {
    /// A machine-checkable artefact.
    Deterministic,
    /// A judged predicate (`followed_predicate_ref` required).
    Judged,
    /// No check claimed.
    None,
}

/// The expiry/invalidation condition kinds (§3.2.8): `model-version-change | date |
/// probe-failure | evidence-refresh-due | experiment-ref`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpiryKind {
    /// The rule must be re-validated when the bound model version changes.
    ModelVersionChange,
    /// The rule expires at a date (`value` carries the date spelling).
    Date,
    /// The rule expires when its probe fails.
    ProbeFailure,
    /// The rule's evidence must be refreshed (`value` carries the refresh interval/date).
    EvidenceRefreshDue,
    /// The rule is valid for the named experiment only (`value` carries the ref).
    ExperimentRef,
}

impl ExpiryKind {
    /// Canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            ExpiryKind::ModelVersionChange => "model_version_change",
            ExpiryKind::Date => "date",
            ExpiryKind::ProbeFailure => "probe_failure",
            ExpiryKind::EvidenceRefreshDue => "evidence_refresh_due",
            ExpiryKind::ExperimentRef => "experiment_ref",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "model_version_change" => ExpiryKind::ModelVersionChange,
            "date" => ExpiryKind::Date,
            "probe_failure" => ExpiryKind::ProbeFailure,
            "evidence_refresh_due" => ExpiryKind::EvidenceRefreshDue,
            "experiment_ref" => ExpiryKind::ExperimentRef,
            _ => return None,
        })
    }
}

/// The debt status (§3.2.8). `Expired` is a state *recorded on the record* (compiled-out
/// artifacts carry it and it lands in `lcd_report.conditioned_rules`; it never silently
/// weakens a rule — compiling under it requires an explicit recorded intent, ADR-0020 §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebtStatus {
    /// The debt is open and within its expiry condition.
    Active,
    /// Expiry is imminent (the `expiring` signal).
    Expiring,
    /// Past the expiry condition.
    Expired,
    /// The debt has been discharged.
    Retired,
}

impl DebtStatus {
    /// Canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            DebtStatus::Active => "active",
            DebtStatus::Expiring => "expiring",
            DebtStatus::Expired => "expired",
            DebtStatus::Retired => "retired",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "active" => DebtStatus::Active,
            "expiring" => DebtStatus::Expiring,
            "expired" => DebtStatus::Expired,
            "retired" => DebtStatus::Retired,
            _ => return None,
        })
    }
}

/// The assumption-debt record (§3.2.8) — the one CF-049 field shape
/// `{rule_id, hypothesis, evidence_refs, owner, expiry_condition{kind, value?},
/// removal_test_ref, status}` used for profile rules, metered `ext` blocks, conditioned
/// variant rules, and definition `HarnessRule.conditioned_on` debts (T-LCD-05, ADR-0020 §6:
/// the *vocabulary* differs across homes; the field shape does not).
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileDebtRecord {
    /// The rule this debt belongs to.
    pub rule_id: String,
    /// The assumption — non-empty free text (kernel-owned `Text` on the wire).
    pub hypothesis: String,
    /// Evidence refs (idp/1 addresses or registry coordinates).
    pub evidence_refs: Vec<String>,
    /// The accountable owner.
    pub owner: String,
    /// The expiry/invalidation condition.
    pub expiry_condition: ExpiryCondition,
    /// The test that would discharge the debt.
    pub removal_test_ref: String,
    /// The debt's status.
    pub status: DebtStatus,
}

/// `expiry_condition{kind, value?}`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExpiryCondition {
    /// The closed kind.
    pub kind: ExpiryKind,
    /// The kind's operand (date spelling / experiment ref / refresh interval).
    pub value: Option<String>,
}

impl ProfileDebtRecord {
    /// Debt completeness (AC-CP-06/T-LCD-05): every field populated and the kind's operand
    /// present where the kind demands one.
    pub fn is_complete(&self) -> bool {
        !self.rule_id.is_empty()
            && !self.hypothesis.is_empty()
            && !self.owner.is_empty()
            && !self.removal_test_ref.is_empty()
            && match self.expiry_condition.kind {
                ExpiryKind::Date | ExpiryKind::EvidenceRefreshDue | ExpiryKind::ExperimentRef => {
                    self.expiry_condition.value.is_some()
                }
                ExpiryKind::ModelVersionChange | ExpiryKind::ProbeFailure => true,
            }
    }

    /// The ADR-0124 §5 admissibility test for a `fallback_profile` debt: a *dated*
    /// hypothesis.
    pub fn is_dated(&self) -> bool {
        self.expiry_condition.kind == ExpiryKind::Date && self.expiry_condition.value.is_some()
    }
}

/// `capabilities` — the typed per-(model, API) declaration (§3.2.3). Fields are the
/// named capability axes; each is a tri-state (`declared` values start here; `probed`
/// arrives only via `probe_profile`, a Stage-3 surface — `unknown` is never coerced).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityState {
    /// The profile declares the capability.
    Declared,
    /// A `probe_profile` conformance record confirmed it (Stage 3).
    Probed,
    /// Unknown — never coerced (AC-CP-04).
    Unknown,
}

impl CapabilityState {
    /// Canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            CapabilityState::Declared => "declared",
            CapabilityState::Probed => "probed",
            CapabilityState::Unknown => "unknown",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "declared" => CapabilityState::Declared,
            "probed" => CapabilityState::Probed,
            "unknown" => CapabilityState::Unknown,
            _ => return None,
        })
    }
}

/// `reasoning_replay{field, opaque}` — which replay field the API exposes and whether
/// its payload is opaque (`opaque` is tri-state like every declared axis; `unsupported`
/// at C0).
#[derive(Debug, Clone, PartialEq)]
pub struct ReasoningReplayDecl {
    /// The replay field name.
    pub field: String,
    /// Whether the replay payload is opaque.
    pub opaque: CapabilityState,
}

impl Default for ReasoningReplayDecl {
    fn default() -> Self {
        ReasoningReplayDecl {
            field: String::new(),
            opaque: CapabilityState::Unknown,
        }
    }
}

/// The named `capabilities` declaration — `CapabilityDeclaration` (§3.2.3). Tri-state
/// axes are `declared` values; `probed` arrives only via `probe_profile` (a Stage-3
/// surface — `unknown` is never coerced). `grammar_tools`/`reasoning_replay` are
/// `unsupported` at C0. Measured bounds (`context_window`, `max_output`) and the
/// vocabularies (`tool_result_role ∈ {native|tool|user|developer}`,
/// `reasoning_levels: level_strings[]`, `usage_mapping`,
/// `compatibility_token{value, provenance}` — never read as authority) are carried as
/// declared data.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileCapabilities {
    /// `native_function_calling` — C0 requires `true` (a non-native-fc interaction mode
    /// is `UnexpressibleSurface`).
    pub native_function_calling: CapabilityState,
    /// `parallel_tool_calls`.
    pub parallel_tool_calls: CapabilityState,
    /// `strict_schema_dialect`.
    pub strict_schema_dialect: CapabilityState,
    /// `structured_output`.
    pub structured_output: CapabilityState,
    /// `grammar_tools` — unsupported at C0.
    pub grammar_tools: CapabilityState,
    /// `reasoning_replay{field, opaque}` — unsupported at C0.
    pub reasoning_replay: ReasoningReplayDecl,
    /// `interleaved_reasoning`.
    pub interleaved_reasoning: CapabilityState,
    /// `image_input`.
    pub image_input: CapabilityState,
    /// `context_window` — a measured bound, not a tri-state.
    pub context_window: Option<u64>,
    /// `max_output` — a measured bound.
    pub max_output: Option<u64>,
    /// `developer_role`.
    pub developer_role: CapabilityState,
    /// `cache_control_convention`.
    pub cache_control_convention: CapabilityState,
    /// `tool_result_name_required`.
    pub tool_result_name_required: CapabilityState,
    /// `assistant_required_after_tool_result`.
    pub assistant_required_after_tool_result: CapabilityState,
    /// `temperature_supported`.
    pub temperature_supported: CapabilityState,
    /// `deferred_tools`.
    pub deferred_tools: CapabilityState,
    /// `tool_search`.
    pub tool_search: CapabilityState,
    /// `seed_honoured` — `Unknown` unless probed.
    pub seed_honoured: CapabilityState,
    /// `substitution_allowed`.
    pub substitution_allowed: CapabilityState,
    /// `compatibility_token{value, provenance}` — never read as authority.
    pub compatibility_token: Option<Json>,
    /// `usage_mapping` — the declared token-usage field mapping.
    pub usage_mapping: Option<Json>,
    /// `tool_result_role ∈ {native | tool | user | developer}` — a declared vocabulary
    /// value, not a tri-state.
    pub tool_result_role: String,
    /// `reasoning_levels` — the declared `level_strings[]`.
    pub reasoning_levels: Vec<String>,
}

impl Default for ProfileCapabilities {
    fn default() -> Self {
        ProfileCapabilities {
            native_function_calling: CapabilityState::Unknown,
            parallel_tool_calls: CapabilityState::Unknown,
            strict_schema_dialect: CapabilityState::Unknown,
            structured_output: CapabilityState::Unknown,
            // Unsupported at C0 (§3.2.3) — spelled `unknown`, never coerced.
            grammar_tools: CapabilityState::Unknown,
            reasoning_replay: ReasoningReplayDecl::default(),
            interleaved_reasoning: CapabilityState::Unknown,
            image_input: CapabilityState::Unknown,
            context_window: None,
            max_output: None,
            developer_role: CapabilityState::Unknown,
            cache_control_convention: CapabilityState::Unknown,
            tool_result_name_required: CapabilityState::Unknown,
            assistant_required_after_tool_result: CapabilityState::Unknown,
            temperature_supported: CapabilityState::Unknown,
            deferred_tools: CapabilityState::Unknown,
            tool_search: CapabilityState::Unknown,
            seed_honoured: CapabilityState::Unknown,
            substitution_allowed: CapabilityState::Unknown,
            compatibility_token: None,
            usage_mapping: None,
            tool_result_role: String::new(),
            reasoning_levels: Vec::new(),
        }
    }
}

/// `version_pattern ∈ {exact | prefix | range | any}` (§3.2.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionPattern {
    /// An exact model version.
    Exact(String),
    /// A version prefix.
    Prefix(String),
    /// A half-open range `{lo, hi}`.
    Range(String, String),
    /// Any version.
    Any,
}

impl VersionPattern {
    /// Specificity for tie-breaking — `exact > range > prefix > any`.
    pub fn specificity(&self) -> u8 {
        match self {
            VersionPattern::Exact(_) => 3,
            VersionPattern::Range(..) => 2,
            VersionPattern::Prefix(_) => 1,
            VersionPattern::Any => 0,
        }
    }
}

/// The selector (§3.2.3): `{provider_api_family, model_family, version_pattern,
/// precedence, successor_ref?, retirement_at?, roles_admitted}`.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileSelector {
    /// The provider API family the profile applies to.
    pub provider_api_family: String,
    /// The model family.
    pub model_family: String,
    /// The version pattern.
    pub version_pattern: VersionPattern,
    /// Chain-order precedence (higher wins ties at equal specificity).
    pub precedence: i64,
    /// The successor profile coordinate, if declared.
    pub successor_ref: Option<String>,
    /// Retirement marker (`ladder.retirement_at` — not "deprecated"; retirement does not
    /// block: expired-at-this-version is distinct from forbidden).
    pub retirement_at: Option<String>,
    /// The closed role set this profile may bind.
    pub roles_admitted: Vec<ModelRole>,
}

/// `roles_admitted` — the closed role vocabulary (§3.2.3 per-role binding).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModelRole {
    /// The main loop model.
    Primary,
    /// A cheap auxiliary call.
    Utility,
    /// Context compaction.
    Compaction,
    /// Sub-agent delegation.
    Subagent,
    /// A judge/verifier model.
    Judge,
    /// Router prediction.
    RouterPredictor,
}

impl ModelRole {
    /// Canonical spelling.
    pub fn name(self) -> &'static str {
        match self {
            ModelRole::Primary => "primary",
            ModelRole::Utility => "utility",
            ModelRole::Compaction => "compaction",
            ModelRole::Subagent => "subagent",
            ModelRole::Judge => "judge",
            ModelRole::RouterPredictor => "router_predictor",
        }
    }

    /// Parse a spelling.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "primary" => ModelRole::Primary,
            "utility" => ModelRole::Utility,
            "compaction" => ModelRole::Compaction,
            "subagent" => ModelRole::Subagent,
            "judge" => ModelRole::Judge,
            "router_predictor" => ModelRole::RouterPredictor,
            _ => return None,
        })
    }
}

/// A typed, metered `ext` block — `{vendor-prefix: typed block}` where the block carries
/// its own debt record (§3.2.3; the same CF-049 shape).
#[derive(Debug, Clone, PartialEq)]
pub struct ExtBlock {
    /// The extension payload (typed within the extension owner's schema).
    pub block: Json,
    /// The block's own debt record.
    pub debt: ProfileDebtRecord,
}

/// `compatibility{inventory_version, min_compiler_version}` (§3.2.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileCompatibility {
    /// The capability-inventory version this profile was written against.
    pub inventory_version: String,
    /// The minimum compiler version that can consume this profile.
    pub min_compiler_version: String,
}

/// `ModelProfile/1` — the profile record (§3.2.3).
#[derive(Debug, Clone, PartialEq)]
pub struct ModelProfile {
    /// `profile_id` — the semantic coordinate.
    pub profile_id: String,
    /// `version` — the profile version.
    pub version: String,
    /// `content_hash` — the `idp/1` content address of the record minus `content_hash`.
    pub content_hash: String,
    /// The binding selector.
    pub selector: ProfileSelector,
    /// `extends` — the parent profile coordinate (chain base → family → version).
    pub extends: Option<String>,
    /// The capability declaration.
    pub capabilities: ProfileCapabilities,
    /// The rule set.
    pub rules: Vec<ProfileRule>,
    /// Sealed `ext` — typed + metered, each block carrying its own debt record.
    pub ext: BTreeMap<String, ExtBlock>,
    /// The profile's own debt record (`expiry` — the record itself is assumed-debt).
    pub expiry: ProfileDebtRecord,
    /// The compatibility pins.
    pub compatibility: ProfileCompatibility,
    /// `tests{…}` — the profile's test-obligations record (§3.2.3): the static /
    /// probe / equivalence / behavioural suite coordinates. Carried as data at
    /// C0/Stage 1; the §5b compliance machinery owns its semantics (Stage 5).
    pub tests: Json,
}

/// `ProfileRule{rule_id, kind, owned_fields[], params, debt, scope?, supersedes?,
/// compliance}` (§3.2.3).
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileRule {
    /// The stable rule id.
    pub rule_id: String,
    /// The closed kind.
    pub kind: ProfileRuleKind,
    /// The fields this rule owns (the per-field merge policy consults this set).
    pub owned_fields: Vec<String>,
    /// Rule parameters.
    pub params: Json,
    /// The rule's debt record.
    pub debt: ProfileDebtRecord,
    /// Optional scope restriction.
    pub scope: Option<Json>,
    /// The rule this rule supersedes.
    pub supersedes: Option<String>,
    /// The compliance claim.
    pub compliance: Compliance,
}

/// The profile's per-field merge policy (§3.2.3: `override | append | forbid_override` —
/// the vocabulary is the profile's own, distinct from the §3.3 layer `MergePolicy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileMergePolicy {
    /// The child's value replaces the parent's.
    Override,
    /// The child's value is appended to the parent's (set union).
    Append,
    /// A child may not set the field at all.
    ForbidOverride,
}

/// The declared per-field merge policy over the `ModelProfile/1` top-level members —
/// longest-prefix table as data, same discipline as `hh_assembly::MERGE_POLICIES`
/// (CC1: one declared policy; a field without a row is a schema bug).
pub const PROFILE_MERGE_POLICIES: &[(&str, ProfileMergePolicy)] = &[
    ("/profile_id", ProfileMergePolicy::Override),
    ("/version", ProfileMergePolicy::Override),
    ("/content_hash", ProfileMergePolicy::Override),
    ("/selector", ProfileMergePolicy::Override),
    ("/extends", ProfileMergePolicy::Override),
    ("/capabilities", ProfileMergePolicy::Override),
    ("/rules", ProfileMergePolicy::Append),
    ("/ext", ProfileMergePolicy::Append),
    ("/expiry", ProfileMergePolicy::Override),
    ("/compatibility", ProfileMergePolicy::Override),
    ("/tests", ProfileMergePolicy::Override),
];

/// The merge policy for a `ModelProfile/1` top-level member name.
pub fn profile_merge_policy(member: &str) -> ProfileMergePolicy {
    let path = format!("/{member}");
    PROFILE_MERGE_POLICIES
        .iter()
        .find(|(p, _)| *p == path)
        .map(|(_, m)| *m)
        .expect("PROFILE_MERGE_POLICIES covers every ModelProfile/1 member")
}

/// The profile read seam at link (§3.2.2): `profile_refs[]` are *coordinates*; the view
/// reads a record by coordinate. The compiler never reaches a live registry — the view is
/// the snapshot-confined read model the caller supplies (CF-046/047 discipline).
pub trait ProfileView {
    /// Read a `ModelProfile/1` record by its coordinate (`profile_id@version` or
    /// `content_hash`); `None` when the coordinate names nothing in the view.
    fn profile(&self, coordinate: &str) -> Option<ModelProfile>;
}

/// Coordinate for a profile — `profile_id@version`.
pub fn profile_coordinate(profile: &ModelProfile) -> String {
    format!("{}@{}", profile.profile_id, profile.version)
}

/// `identity(p) = h(p minus content_hash)` — the `content_hash` member is the `idp/1`
/// address of the rest (§3.2.3).
pub fn profile_identity(profile: &ModelProfile) -> String {
    let mut p = profile.clone();
    p.content_hash = String::new();
    let mut j = crate::schema::profile_to_json(&p);
    // Normalise the absent member the same way every time.
    if let Json::Obj(m) = &mut j {
        m.remove("content_hash");
    }
    hh_identity::idp_id("model_profile.1", j.to_canonical_string().as_bytes())
}

/// The union of fields a profile's rules own — the set a child's overrides may touch
/// (`{identity members} ∪ rules[].owned_fields`); structural members a version profile
/// always owns are listed here so the check is spelled out once.
pub fn owned_fields(profile: &ModelProfile) -> std::collections::BTreeSet<String> {
    let mut s: std::collections::BTreeSet<String> = profile
        .rules
        .iter()
        .flat_map(|r| r.owned_fields.iter().cloned())
        .collect();
    for m in [
        "profile_id",
        "version",
        "content_hash",
        "selector",
        "extends",
        "rules",
        "ext",
        "expiry",
        "compatibility",
        "capabilities",
        "tests",
    ] {
        s.insert(m.to_string());
    }
    s
}

/// Diff the top-level members of two profiles; returns the member names whose canonical
/// values differ. Used by the chain `overrides ⊆ owned_fields` check.
pub fn member_diff(parent: &ModelProfile, child: &ModelProfile) -> Vec<String> {
    let pj = crate::schema::profile_to_json(parent);
    let cj = crate::schema::profile_to_json(child);
    let (Json::Obj(pm), Json::Obj(cm)) = (&pj, &cj) else {
        return Vec::new();
    };
    let mut names: std::collections::BTreeSet<String> = pm.keys().cloned().collect();
    names.extend(cm.keys().cloned());
    names
        .into_iter()
        .filter(|k| pm.get(k) != cm.get(k))
        .collect()
}

/// `extends`-chain resolution: walk `extends` from the bound coordinates to the root,
/// order base→version, and refuse a cycle or a chain deeper than the C0 two-level bound
/// (§3.2.2: "deeper chains are C1/Stage 5"). Siblings order by declared selector
/// precedence then specificity — a tie is `AmbiguousSelector`, resolution never guesses.
pub fn resolve_chain(
    bound: &[String],
    view: &dyn ProfileView,
) -> Result<Vec<ModelProfile>, CompileError> {
    // Collect the reachable set (bound coordinates ∪ `extends` ancestors), keyed by the
    // canonical coordinate so a content-hash ref and an id@version ref dedupe.
    let mut profiles: BTreeMap<String, ModelProfile> = BTreeMap::new();
    let mut stack: Vec<String> = bound.to_vec();
    while let Some(coord) = stack.pop() {
        let p = match profiles
            .values()
            .find(|p| profile_coordinate(p) == coord || p.content_hash == coord)
        {
            Some(p) => p.clone(),
            None => view
                .profile(&coord)
                .ok_or_else(|| CompileError::LinkError {
                    kind: crate::errors::LinkErrorKind::VersionConflict,
                    detail: format!("profile coordinate {coord} not in the bound view"),
                    diagnostics: Vec::new(),
                })?,
        };
        let key = profile_coordinate(&p);
        if profiles.contains_key(&key) {
            continue;
        }
        if let Some(parent) = &p.extends {
            stack.push(parent.clone());
        }
        profiles.insert(key, p);
    }
    // Topo order — parent before child. Among unconstrained candidates the lowest
    // (precedence, specificity) is emitted first so the higher-precedence profile applies
    // last (its overrides win); a candidate tie is `AmbiguousSelector`.
    let mut remaining: Vec<ModelProfile> = profiles.into_values().collect();
    let mut emitted: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut chain: Vec<ModelProfile> = Vec::new();
    while !remaining.is_empty() {
        let mut candidates: Vec<usize> = remaining
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                p.extends
                    .as_ref()
                    .map(|e| emitted.contains(e))
                    .unwrap_or(true)
            })
            .map(|(i, _)| i)
            .collect();
        if candidates.is_empty() {
            return Err(CompileError::InvalidModelProfile {
                detail: "profile `extends` cycle".to_string(),
            });
        }
        candidates.sort_by_key(|&i| {
            let s = &remaining[i].selector;
            (s.precedence, s.version_pattern.specificity())
        });
        if candidates.len() > 1 {
            let a = &remaining[candidates[0]];
            let b = &remaining[candidates[1]];
            let ka = (
                a.selector.precedence,
                a.selector.version_pattern.specificity(),
            );
            let kb = (
                b.selector.precedence,
                b.selector.version_pattern.specificity(),
            );
            if ka == kb {
                return Err(CompileError::AmbiguousSelector {
                    detail: format!(
                        "{} and {} tie at precedence {} / specificity {}",
                        profile_coordinate(a),
                        profile_coordinate(b),
                        a.selector.precedence,
                        a.selector.version_pattern.specificity()
                    ),
                });
            }
        }
        let p = remaining.remove(candidates[0]);
        emitted.insert(profile_coordinate(&p));
        emitted.insert(p.content_hash.clone());
        chain.push(p);
    }
    // C0 bound: at most two levels (§3.2.2).
    if chain.len() > 2 {
        return Err(CompileError::InvalidModelProfile {
            detail: format!(
                "profile chain depth {} exceeds the C0 two-level bound",
                chain.len()
            ),
        });
    }
    Ok(chain)
}
