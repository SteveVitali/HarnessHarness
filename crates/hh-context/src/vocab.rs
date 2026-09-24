//! The closed vocabularies of §5c (R-2.4.1/R-2.4.3⁰/R-2.4.4⁰): `CandidateKind`,
//! `PriorityClass`, `Retention`, `CacheTier`, `OmissionReason`, the memory sums
//! (`MemoryKind`, `LifecycleState`, `CacheHint`, `Revalidation`,
//! `DependencyKind`, `InvalidationCondition`, `ConflictPolicy`,
//! `RevocationReason`) and the `RetrievalQuery` sum. Every sum is closed —
//! growth is a dialect bump, never an ad-hoc string (CC7/CC8).

use hh_provenance::PersistenceScope;
use hh_wire::json::Json;

use crate::codec::{arr_at, expect_obj, int_at, reject_unknown, str_at, str_set_at};
use crate::CodecError;

// ─────────────────────────────────────────────────────────────────────────────
// §5c.1 context-builder vocabulary
// ─────────────────────────────────────────────────────────────────────────────

/// `CandidateKind` — the closed candidate-kind sum (§5c.1 data model; ADR-0072;
/// `memory_index` per CF-176). Never encodes a provider role. Registered `ext`
/// kinds are a dialect bump — the Stage-1 sum is the spec's fourteen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CandidateKind {
    /// A kernel notice (the omission item, current-time items).
    KernelNotice,
    /// A sealed-definition instruction.
    DefinitionInstruction,
    /// The principal's message.
    PrincipalMessage,
    /// A procedure's index form (delivered `handle_only`).
    ProcedureIndex,
    /// A procedure body.
    ProcedureBody,
    /// A tool surface.
    ToolSurface,
    /// A memory body.
    Memory,
    /// A memory index / manifest line.
    MemoryIndex,
    /// A tool/environment observation.
    Observation,
    /// An artifact excerpt.
    ArtifactExcerpt,
    /// A transcript item (paired calls/observations, model outputs).
    TranscriptItem,
    /// A soft-budget advise reminder.
    BudgetReminder,
    /// An environment-state item.
    EnvironmentState,
    /// A subagent result re-entering at ≤ `delegate`.
    SubagentResult,
}

/// The fourteen-member `CandidateKind` table (kernel-fixed).
pub const CANDIDATE_KINDS: &[CandidateKind] = &[
    CandidateKind::KernelNotice,
    CandidateKind::DefinitionInstruction,
    CandidateKind::PrincipalMessage,
    CandidateKind::ProcedureIndex,
    CandidateKind::ProcedureBody,
    CandidateKind::ToolSurface,
    CandidateKind::Memory,
    CandidateKind::MemoryIndex,
    CandidateKind::Observation,
    CandidateKind::ArtifactExcerpt,
    CandidateKind::TranscriptItem,
    CandidateKind::BudgetReminder,
    CandidateKind::EnvironmentState,
    CandidateKind::SubagentResult,
];

/// The kernel-fixed *unconditionally volatile* kinds (§5c.1 LC-3;
/// `volatile_kinds ⊇ {budget_reminder, environment_state, kernel_notice(current_time),
/// occupancy}`). `kernel_notice` is volatile only in its `current_time` form —
/// that qualification is candidate-level ([`crate::plan::Candidate::volatile`]),
/// because the kernel slot must admit the (non-volatile) omission item.
pub const VOLATILE_KINDS: &[CandidateKind] = &[
    CandidateKind::BudgetReminder,
    CandidateKind::EnvironmentState,
];

impl CandidateKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CandidateKind::KernelNotice => "kernel_notice",
            CandidateKind::DefinitionInstruction => "definition_instruction",
            CandidateKind::PrincipalMessage => "principal_message",
            CandidateKind::ProcedureIndex => "procedure_index",
            CandidateKind::ProcedureBody => "procedure_body",
            CandidateKind::ToolSurface => "tool_surface",
            CandidateKind::Memory => "memory",
            CandidateKind::MemoryIndex => "memory_index",
            CandidateKind::Observation => "observation",
            CandidateKind::ArtifactExcerpt => "artifact_excerpt",
            CandidateKind::TranscriptItem => "transcript_item",
            CandidateKind::BudgetReminder => "budget_reminder",
            CandidateKind::EnvironmentState => "environment_state",
            CandidateKind::SubagentResult => "subagent_result",
        }
    }

    /// Parse the canonical spelling (`None` = unregistered kind).
    pub fn parse(s: &str) -> Option<CandidateKind> {
        CANDIDATE_KINDS.iter().copied().find(|k| k.as_str() == s)
    }
}

/// `PriorityClass` — the kernel-fixed closed set the eviction order reads
/// (§5c.1; ADR-0073 d1; `memory_index` ordered after `memory` per CF-176).
/// Declaration order **is** the kernel eviction order: `Commentary` evicts
/// first, `TranscriptTail` last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PriorityClass {
    /// Commentary / notices — evicted first.
    Commentary,
    /// Old observations.
    ObservationOld,
    /// Recent observations.
    ObservationRecent,
    /// Images.
    Image,
    /// Memories.
    Memory,
    /// Memory index lines — ordered after `memory` (CF-176).
    MemoryIndex,
    /// Procedure bodies.
    ProcedureBody,
    /// The transcript tail — evicted last.
    TranscriptTail,
}

/// The kernel-fixed `PriorityClass` table in eviction order (ascending rank).
pub const PRIORITY_CLASSES: &[PriorityClass] = &[
    PriorityClass::Commentary,
    PriorityClass::ObservationOld,
    PriorityClass::ObservationRecent,
    PriorityClass::Image,
    PriorityClass::Memory,
    PriorityClass::MemoryIndex,
    PriorityClass::ProcedureBody,
    PriorityClass::TranscriptTail,
];

impl PriorityClass {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            PriorityClass::Commentary => "commentary",
            PriorityClass::ObservationOld => "observation_old",
            PriorityClass::ObservationRecent => "observation_recent",
            PriorityClass::Image => "image",
            PriorityClass::Memory => "memory",
            PriorityClass::MemoryIndex => "memory_index",
            PriorityClass::ProcedureBody => "procedure_body",
            PriorityClass::TranscriptTail => "transcript_tail",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<PriorityClass> {
        PRIORITY_CLASSES.iter().copied().find(|p| p.as_str() == s)
    }

    /// The kernel eviction rank (0 evicts first).
    pub fn eviction_rank(self) -> u64 {
        PRIORITY_CLASSES.iter().position(|p| *p == self).unwrap() as u64
    }
}

/// `retention ∈ {required, optional(PriorityClass)}` (§5c.1; ADR-0073).
/// `Required` is set only by the kernel (reserved-slot content, the principal's
/// current message, a paired current-turn observation, the omission item) or a
/// definition-level `HarnessRule` — never by a policy (I-NOWIDEN).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retention {
    /// Kernel-required — never truncated or evicted.
    Required,
    /// Evictable at the declared priority class.
    Optional(PriorityClass),
}

impl Retention {
    /// `true` when kernel-required.
    pub fn is_required(self) -> bool {
        matches!(self, Retention::Required)
    }
}

/// `Candidate.state ∈ {handle_only, expanded}` (§5c.1; ADR-0073 d6).
/// `handle_only ⇔ indexed` — the index form renders; expansion is an effect
/// through a declared read capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateState {
    /// The index form only (the body is reached through `read_capability`).
    HandleOnly,
    /// The expanded form.
    Expanded,
}

impl CandidateState {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CandidateState::HandleOnly => "handle_only",
            CandidateState::Expanded => "expanded",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<CandidateState> {
        match s {
            "handle_only" => Some(CandidateState::HandleOnly),
            "expanded" => Some(CandidateState::Expanded),
            _ => None,
        }
    }
}

/// `cache_tier ∈ {static, dynamic, transcript}` (§5c.1 LC-1; ADR-0127). The
/// declaration order `Static < Dynamic < Transcript` is the tier order the
/// LC-1 link check reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CacheTier {
    /// Byte-identical across consecutive same-purpose calls (LC-2).
    Static,
    /// Dynamic.
    Dynamic,
    /// The transcript tier.
    Transcript,
}

impl CacheTier {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CacheTier::Static => "static",
            CacheTier::Dynamic => "dynamic",
            CacheTier::Transcript => "transcript",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<CacheTier> {
        match s {
            "static" => Some(CacheTier::Static),
            "dynamic" => Some(CacheTier::Dynamic),
            "transcript" => Some(CacheTier::Transcript),
            _ => None,
        }
    }

    /// The tier order — `static < dynamic < transcript` (LC-1).
    pub fn rank(self) -> u64 {
        match self {
            CacheTier::Static => 0,
            CacheTier::Dynamic => 1,
            CacheTier::Transcript => 2,
        }
    }
}

/// `Omission.reason` — the closed set (§5c.1 data model; ADR-0072 d2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OmissionReason {
    /// No slot's `min_authority` admitted the item (I-RP).
    Authority,
    /// The lifecycle state was never admissible (I-ORDER).
    Validity,
    /// The readers check excluded the reader (carried C0, enforced C2).
    Readers,
    /// Evicted by `enforce_budget` or deselected under budget pressure.
    Budget,
    /// A duplicate of an admitted item.
    Dedup,
    /// Media the renderer cannot carry (I-MEDIA).
    MediaUnsupported,
    /// Over the slot's cardinality.
    Cardinality,
    /// No compiled identity or content address (I-ID).
    Unidentified,
    /// A checkable precondition false at delivery.
    Precondition,
}

impl OmissionReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            OmissionReason::Authority => "authority",
            OmissionReason::Validity => "validity",
            OmissionReason::Readers => "readers",
            OmissionReason::Budget => "budget",
            OmissionReason::Dedup => "dedup",
            OmissionReason::MediaUnsupported => "media_unsupported",
            OmissionReason::Cardinality => "cardinality",
            OmissionReason::Unidentified => "unidentified",
            OmissionReason::Precondition => "precondition",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<OmissionReason> {
        [
            OmissionReason::Authority,
            OmissionReason::Validity,
            OmissionReason::Readers,
            OmissionReason::Budget,
            OmissionReason::Dedup,
            OmissionReason::MediaUnsupported,
            OmissionReason::Cardinality,
            OmissionReason::Unidentified,
            OmissionReason::Precondition,
        ]
        .into_iter()
        .find(|r| r.as_str() == s)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// §5c.3/§5c.4 memory vocabulary
// ─────────────────────────────────────────────────────────────────────────────

/// `MemoryKind` — the one closed sum per HIR/1 dialect (§5c.3 data model;
/// ADR-0078 d6 as amended; CF-473). A ranker feature, a manifest attribute and
/// the key of `MemoryPolicy.per_kind_defaults`; it never decides authority,
/// validity or readers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemoryKind {
    /// A fact.
    Fact,
    /// A preference.
    Preference,
    /// An episode (the episodic layer's distilled form).
    Episode,
    /// A diagnosis.
    Diagnosis,
    /// A recovery note.
    Recovery,
    /// A procedure pointer (the P-layer index form).
    ProcedurePointer,
    /// A summary.
    Summary,
    /// A journal entry.
    Journal,
    /// A reference.
    Reference,
    /// A decision.
    Decision,
    /// A tool-result cache entry (ADR-0129 D1).
    ToolResult,
}

/// The eleven-member `MemoryKind` table.
pub const MEMORY_KINDS: &[MemoryKind] = &[
    MemoryKind::Fact,
    MemoryKind::Preference,
    MemoryKind::Episode,
    MemoryKind::Diagnosis,
    MemoryKind::Recovery,
    MemoryKind::ProcedurePointer,
    MemoryKind::Summary,
    MemoryKind::Journal,
    MemoryKind::Reference,
    MemoryKind::Decision,
    MemoryKind::ToolResult,
];

impl MemoryKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryKind::Fact => "fact",
            MemoryKind::Preference => "preference",
            MemoryKind::Episode => "episode",
            MemoryKind::Diagnosis => "diagnosis",
            MemoryKind::Recovery => "recovery",
            MemoryKind::ProcedurePointer => "procedure_pointer",
            MemoryKind::Summary => "summary",
            MemoryKind::Journal => "journal",
            MemoryKind::Reference => "reference",
            MemoryKind::Decision => "decision",
            MemoryKind::ToolResult => "tool_result",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<MemoryKind> {
        MEMORY_KINDS.iter().copied().find(|k| k.as_str() == s)
    }
}

/// `LifecycleStateKind` — the unit sum `admitted_states` subsets draw from
/// (§5c.4; `superseded | revoked | expired` are never admissible in
/// `mode = execute` and never in a `SlotDeclaration` — `InvalidLayout` at link).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LifecycleStateKind {
    /// `valid`.
    Valid,
    /// `superseded`.
    Superseded,
    /// `revoked`.
    Revoked,
    /// `expired`.
    Expired,
    /// `stale_by_dependency`.
    StaleByDependency,
    /// `unknown`.
    Unknown,
}

impl LifecycleStateKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            LifecycleStateKind::Valid => "valid",
            LifecycleStateKind::Superseded => "superseded",
            LifecycleStateKind::Revoked => "revoked",
            LifecycleStateKind::Expired => "expired",
            LifecycleStateKind::StaleByDependency => "stale_by_dependency",
            LifecycleStateKind::Unknown => "unknown",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<LifecycleStateKind> {
        [
            LifecycleStateKind::Valid,
            LifecycleStateKind::Superseded,
            LifecycleStateKind::Revoked,
            LifecycleStateKind::Expired,
            LifecycleStateKind::StaleByDependency,
            LifecycleStateKind::Unknown,
        ]
        .into_iter()
        .find(|k| k.as_str() == s)
    }
}

/// `lifecycle_state(v, at)` — the computed state (§5c.4; ADR-0081 d1), carrying
/// its evidence. Pure over records alone (V-DET): `supersedes` edges into `v`,
/// `RevocationRecord`s naming `v`, `v.validity`, `check_contract(v.contract,
/// at)` and the `MemoryStaleIndex`; precedence
/// `revoked > superseded > expired > stale_by_dependency > valid > unknown`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleState {
    /// Every checkable condition holds.
    Valid,
    /// A `supersedes` edge names `v` as the older member.
    Superseded {
        /// The newer version id.
        by: String,
    },
    /// A `RevocationRecord` names `v`.
    Revoked {
        /// The revoker's rendered authority/origin.
        record: String,
        /// The closed revocation reason.
        reason: RevocationReason,
    },
    /// The validity window/freshness elapsed or an invalidation condition fired.
    Expired {
        /// Why (`ttl_elapsed`, `scope_ended`, `validator_fails`, …).
        reason: String,
    },
    /// A justification/dependency transitively revoked or stale (J1).
    StaleByDependency {
        /// The revoked/stale inputs.
        revoked_inputs: Vec<String>,
    },
    /// The contract declares nothing checkable.
    Unknown,
}

impl LifecycleState {
    /// The unit kind (what `admitted_states` subsets name).
    pub fn kind(&self) -> LifecycleStateKind {
        match self {
            LifecycleState::Valid => LifecycleStateKind::Valid,
            LifecycleState::Superseded { .. } => LifecycleStateKind::Superseded,
            LifecycleState::Revoked { .. } => LifecycleStateKind::Revoked,
            LifecycleState::Expired { .. } => LifecycleStateKind::Expired,
            LifecycleState::StaleByDependency { .. } => LifecycleStateKind::StaleByDependency,
            LifecycleState::Unknown => LifecycleStateKind::Unknown,
        }
    }

    /// `true` for `valid`.
    pub fn is_valid(&self) -> bool {
        matches!(self, LifecycleState::Valid)
    }
}

/// `cache_hint ∈ {cacheable, recompute, no_store}` (§5c.4 `InvalidationContract`).
/// `recompute` memories never persist beyond `turn`; `no_store` never persists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheHint {
    /// Persist normally.
    Cacheable,
    /// Do not persist beyond `turn`.
    Recompute,
    /// Never persist.
    NoStore,
}

impl CacheHint {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            CacheHint::Cacheable => "cacheable",
            CacheHint::Recompute => "recompute",
            CacheHint::NoStore => "no_store",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<CacheHint> {
        match s {
            "cacheable" => Some(CacheHint::Cacheable),
            "recompute" => Some(CacheHint::Recompute),
            "no_store" => Some(CacheHint::NoStore),
            _ => None,
        }
    }
}

/// `revalidation ∈ {must_revalidate, stale_ok{max_stale}, never}` (§5c.4).
/// `must_revalidate` makes an elapsed freshness `expired` immediately;
/// `stale_ok{max_stale}` admits only as `unknown`, never as `valid`; `never`
/// takes freshness out of the checkable set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Revalidation {
    /// Elapsed freshness ⇒ `expired`.
    MustRevalidate,
    /// Elapsed freshness ⇒ `unknown` (never `valid`) within `max_stale`.
    StaleOk {
        /// The bound on stale reads.
        max_stale: u64,
    },
    /// Freshness never gates validity.
    Never,
}

/// `granularity ∈ {row, table}` — a `table` stamp is legal though a kind's
/// default policy may forbid it (`MemoryPolicy.per_kind_defaults` carries
/// `granularity_allowed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    /// One record's stamp.
    Row,
    /// The whole table's stamp.
    Table,
}

impl Granularity {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Granularity::Row => "row",
            Granularity::Table => "table",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<Granularity> {
        match s {
            "row" => Some(Granularity::Row),
            "table" => Some(Granularity::Table),
            _ => None,
        }
    }
}

/// `DependencyKind` — the closed set of dependency stamps (§5c.4; ADR-0081 d2
/// as amended CF-277; OQ-206 resolved). Adding a kind is a dialect bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DependencyKind {
    /// A registry logical name.
    RegistryName,
    /// The sealed definition's version.
    DefinitionVersion,
    /// A procedure version.
    ProcedureVersion,
    /// A tool capability version.
    ToolCapabilityVersion,
    /// An environment image.
    EnvironmentImage,
    /// An environment mutation epoch `{environment_ref, mutation_epoch}`.
    EnvironmentEpoch,
    /// A model snapshot (`authority = external` claim).
    ModelSnapshot,
    /// A profile version (the `profile_version` K5 contract member,
    /// §5b.4 K5 entry — ADR-0129 d.2).
    ProfileVersion,
    /// Another memory version.
    MemoryVersion,
    /// An artifact (content address).
    Artifact,
    /// An external resource (validator-gated).
    ExternalResource,
}

impl DependencyKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            DependencyKind::RegistryName => "registry_name",
            DependencyKind::DefinitionVersion => "definition_version",
            DependencyKind::ProcedureVersion => "procedure_version",
            DependencyKind::ToolCapabilityVersion => "tool_capability_version",
            DependencyKind::EnvironmentImage => "environment_image",
            DependencyKind::EnvironmentEpoch => "environment_epoch",
            DependencyKind::ModelSnapshot => "model_snapshot",
            DependencyKind::ProfileVersion => "profile_version",
            DependencyKind::MemoryVersion => "memory_version",
            DependencyKind::Artifact => "artifact",
            DependencyKind::ExternalResource => "external_resource",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<DependencyKind> {
        [
            DependencyKind::RegistryName,
            DependencyKind::DefinitionVersion,
            DependencyKind::ProcedureVersion,
            DependencyKind::ToolCapabilityVersion,
            DependencyKind::EnvironmentImage,
            DependencyKind::EnvironmentEpoch,
            DependencyKind::ModelSnapshot,
            DependencyKind::ProfileVersion,
            DependencyKind::MemoryVersion,
            DependencyKind::Artifact,
            DependencyKind::ExternalResource,
        ]
        .into_iter()
        .find(|k| k.as_str() == s)
    }
}

/// `invalidation_condition` — the closed set (§5c.4 `InvalidationContract`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidationCondition {
    /// A declared dependency stamp changed.
    DependencyChanged,
    /// The TTL elapsed.
    TtlElapsed,
    /// The validator's last verdict failed.
    ValidatorFails,
    /// A successor was published.
    Superseded,
    /// The named scope ended.
    ScopeEnded(PersistenceScope),
    /// A replacement was published under `name`.
    ReplacementPublished(String),
}

impl InvalidationCondition {
    /// The canonical kind spelling.
    pub fn kind(&self) -> &'static str {
        match self {
            InvalidationCondition::DependencyChanged => "dependency_changed",
            InvalidationCondition::TtlElapsed => "ttl_elapsed",
            InvalidationCondition::ValidatorFails => "validator_fails",
            InvalidationCondition::Superseded => "superseded",
            InvalidationCondition::ScopeEnded(_) => "scope_ended",
            InvalidationCondition::ReplacementPublished(_) => "replacement_published",
        }
    }
}

/// `conflict_policy ∈ {deliver_all_annotated (default), withhold_all,
/// deliver_head_if_resolved_else_withhold}` (§5c.4; ADR-0082 d5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictPolicy {
    /// Deliver every member annotated with `conflict_set_ref`.
    DeliverAllAnnotated,
    /// Withhold the whole set.
    WithholdAll,
    /// Deliver the resolved head; withhold the set when unresolved.
    DeliverHeadIfResolvedElseWithhold,
}

impl ConflictPolicy {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ConflictPolicy::DeliverAllAnnotated => "deliver_all_annotated",
            ConflictPolicy::WithholdAll => "withhold_all",
            ConflictPolicy::DeliverHeadIfResolvedElseWithhold => {
                "deliver_head_if_resolved_else_withhold"
            }
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<ConflictPolicy> {
        match s {
            "deliver_all_annotated" => Some(ConflictPolicy::DeliverAllAnnotated),
            "withhold_all" => Some(ConflictPolicy::WithholdAll),
            "deliver_head_if_resolved_else_withhold" => {
                Some(ConflictPolicy::DeliverHeadIfResolvedElseWithhold)
            }
            _ => None,
        }
    }
}

/// `ConflictSet.resolution` (§5c.4; ADR-0082 d3–d4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictResolution {
    /// Resolved by an authority-rule-satisfying supersession.
    Superseded {
        /// The head version id.
        head: String,
    },
    /// The members coexist (the lower-authority version never withholds the
    /// higher; the judged tail can only coexist/escalate).
    Coexist,
    /// The set is withheld.
    Withheld,
    /// Escalated to the principal.
    Escalated {
        /// The principal's reference.
        principal_ref: String,
    },
}

/// `RevocationReason` — the closed set (§5c.4 data model; ADR-0082 d1; shared
/// shape with extension revocation). `release` supersedes a `hold` with a new
/// version — it is never an un-revoke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevocationReason {
    /// Contradicted by newer information.
    Contradicted,
    /// The principal withdrew it.
    PrincipalWithdrawn,
    /// Its source was revoked.
    SourceRevoked,
    /// A dependency was revoked.
    DependencyRevoked,
    /// It expired.
    Expired,
    /// It was migrated.
    Migrated,
    /// Poisoned input.
    Poisoned,
    /// Held pending review.
    Hold,
    /// Released from a `hold` (a superseding record — never an un-revoke).
    Release,
}

impl RevocationReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RevocationReason::Contradicted => "contradicted",
            RevocationReason::PrincipalWithdrawn => "principal_withdrawn",
            RevocationReason::SourceRevoked => "source_revoked",
            RevocationReason::DependencyRevoked => "dependency_revoked",
            RevocationReason::Expired => "expired",
            RevocationReason::Migrated => "migrated",
            RevocationReason::Poisoned => "poisoned",
            RevocationReason::Hold => "hold",
            RevocationReason::Release => "release",
        }
    }

    /// Parse the canonical spelling.
    pub fn parse(s: &str) -> Option<RevocationReason> {
        [
            RevocationReason::Contradicted,
            RevocationReason::PrincipalWithdrawn,
            RevocationReason::SourceRevoked,
            RevocationReason::DependencyRevoked,
            RevocationReason::Expired,
            RevocationReason::Migrated,
            RevocationReason::Poisoned,
            RevocationReason::Hold,
            RevocationReason::Release,
        ]
        .into_iter()
        .find(|r| r.as_str() == s)
    }

    /// The `hh-identity` `SupersedeReason` this revocation maps to in the
    /// lineage machine (the coarse §8.3 sum — the precise `RevocationReason`
    /// lives on the memory record and the `context.memory.invalidated` row).
    pub fn lineage_reason(self) -> hh_identity::supersede::SupersedeReason {
        use hh_identity::supersede::SupersedeReason as S;
        match self {
            RevocationReason::Expired => S::Expiry,
            RevocationReason::Migrated => S::Migration,
            _ => S::Revocation,
        }
    }
}

/// `MemoryContent` — `content: Text | StructuredValue` (§5c.3 data model).
/// Free text is R-TEXT-capped at `external`; a closed-schema `StructuredValue`
/// endorsed by a `kernel` validator caps at `environment`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryContent {
    /// A `Text` leaf (`hh-hir`'s — the one prose type, CC7). Boxed: the
    /// leaf is the large variant.
    Text(Box<hh_hir::leaves::Text>),
    /// A closed-schema structured value.
    Structured(Json),
}

impl MemoryContent {
    /// `true` for the free-text arm (R-TEXT applies).
    pub fn is_text(&self) -> bool {
        matches!(self, MemoryContent::Text(_))
    }

    /// The text to index, when the content carries any (a `Text` leaf's raw
    /// content; a structured value contributes nothing at C0 — its searchable
    /// surface is its canonical form).
    pub fn index_text(&self) -> String {
        match self {
            MemoryContent::Text(t) => t.content.clone().unwrap_or_default(),
            MemoryContent::Structured(j) => j.to_canonical_string(),
        }
    }
}

/// `SubjectKey{schema_ref, key}` — the deterministic-conflict key (§5c.4;
/// ADR-0082 d3). `key` is the canonical bytes of the closed-schema subject.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SubjectKey {
    /// `Ref` to the closed schema in the sealed definition.
    pub schema_ref: String,
    /// The canonical-bytes subject key.
    pub key: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// §5c.3 retrieval vocabulary
// ────────────────────────────────────────────────────────────────────────────

/// The retrieval layers — the addressing classes `retrieve` may serve
/// (§5c.3; ADR-0078 d1). `W` (the `ContextPlan` projection) is **never** a
/// retrieval source and so is not a member — a `"w"` spelling decodes to
/// `UnknownLayer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Layer {
    /// Artifact memory — content-addressed blobs + the environment tree.
    Artifact,
    /// Episodic memory — verbatim ranges and distilled episodes.
    Episodic,
    /// Procedural memory — procedure/skill index forms.
    Procedural,
    /// Durable session state — named session artifacts and manifests.
    Session,
}

impl Layer {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Layer::Artifact => "A",
            Layer::Episodic => "E",
            Layer::Procedural => "P",
            Layer::Session => "S",
        }
    }

    /// Parse the canonical spelling (`None` = `UnknownLayer` at the caller).
    pub fn parse(s: &str) -> Option<Layer> {
        match s {
            "A" | "artifact" => Some(Layer::Artifact),
            "E" | "episodic" => Some(Layer::Episodic),
            "P" | "procedural" => Some(Layer::Procedural),
            "S" | "session" => Some(Layer::Session),
            _ => None,
        }
    }
}

/// `lexical.match_mode ∈ {any, all_same_line, all_within{n}}` (§5c.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMode {
    /// Any term hits.
    Any,
    /// All terms on one line.
    AllSameLine,
    /// All terms within `n` tokens of each other.
    AllWithin {
        /// The window.
        n: u64,
    },
}

/// `trigger` kind — `message | path_touched | explicit` (§5c.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerKind {
    /// A model-message trigger.
    Message(String),
    /// A touched path.
    PathTouched(String),
    /// An explicit name.
    Explicit(String),
}

/// `RetrievalQuery` — the closed + registered-ext query-kind sum (§5c.3;
/// ADR-0079). The C0 kinds are executable now; `structural` is declared (C1 —
/// `IndexUnavailable{structural_index}` until OQ-203's index lands) and
/// `similarity` is declared (C2 — `EmbedderUnpinned` without a pinned
/// `ModelSnapshotRecord`; it declares `deterministic = false` by construction).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetrievalQuery {
    /// `by_address` — a content address / version id.
    ByAddress {
        /// The address.
        address: String,
    },
    /// `by_name` — a logical name in the store's name space.
    ByName {
        /// The scope the name lives under.
        scope: PersistenceScope,
        /// The name.
        name: String,
    },
    /// `by_path_glob` — artifact paths under a glob.
    ByPathGlob {
        /// The glob (`*`/`**`/`?` segments).
        pattern: String,
    },
    /// `by_run{run_id, seq_range?, classes?}` — items derived from a run's
    /// ledger range (always content-addressable — R7).
    ByRun {
        /// The run.
        run_id: String,
        /// The inclusive seq range.
        seq_range: Option<(u64, u64)>,
        /// Restrict to event classes.
        classes: Vec<String>,
    },
    /// `lexical{terms, match_mode, context_lines, case_sensitive, normalized}`.
    Lexical {
        /// The query terms.
        terms: Vec<String>,
        /// The match mode.
        match_mode: MatchMode,
        /// Context lines carried on hits.
        context_lines: u64,
        /// Case sensitivity.
        case_sensitive: bool,
        /// Unicode-folded normalized matching.
        normalized: bool,
    },
    /// `discover{path, direction, filenames, ceiling}` — walk for named files.
    Discover {
        /// The path to start from.
        path: String,
        /// `upward` | `jit_downward`.
        direction: DiscoverDirection,
        /// The filenames sought.
        filenames: Vec<String>,
        /// The walk ceiling.
        ceiling: DiscoverCeiling,
    },
    /// `trigger{message | path_touched | explicit}`.
    Trigger {
        /// The trigger.
        kind: TriggerKind,
    },
    /// `structural{anchors, mentioned_idents}` — **C1** (declared; the C0 store
    /// answers `IndexUnavailable{structural_index}` — OQ-203).
    Structural {
        /// Anchor refs.
        anchors: Vec<String>,
        /// Mentioned identifiers.
        mentioned_idents: Vec<String>,
    },
    /// `similarity{text, embedder: ProfileRef}` — **C2** (declared;
    /// `deterministic = false` by construction and refused `EmbedderUnpinned`
    /// without a pinned snapshot — ADR-0120).
    Similarity {
        /// The probe text.
        text: String,
        /// The embedder profile ref.
        embedder: String,
    },
}

/// `discover.direction ∈ {upward, jit_downward}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverDirection {
    /// Walk toward the ceiling.
    Upward,
    /// Just-in-time downward walk.
    JitDownward,
}

/// `discover.ceiling ∈ {workspace_root, git_root}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverCeiling {
    /// Stop at the workspace root.
    WorkspaceRoot,
    /// Stop at the git root.
    GitRoot,
}

impl RetrievalQuery {
    /// The closed kind spelling (`context.retrieval.completed.query_kind`).
    pub fn kind(&self) -> &'static str {
        match self {
            RetrievalQuery::ByAddress { .. } => "by_address",
            RetrievalQuery::ByName { .. } => "by_name",
            RetrievalQuery::ByPathGlob { .. } => "by_path_glob",
            RetrievalQuery::ByRun { .. } => "by_run",
            RetrievalQuery::Lexical { .. } => "lexical",
            RetrievalQuery::Discover { .. } => "discover",
            RetrievalQuery::Trigger { .. } => "trigger",
            RetrievalQuery::Structural { .. } => "structural",
            RetrievalQuery::Similarity { .. } => "similarity",
        }
    }

    /// Whether the kind is a deterministic function of store state at `at`
    /// (§5c.3: every kind but `similarity` is pure).
    pub fn deterministic(&self) -> bool {
        !matches!(self, RetrievalQuery::Similarity { .. })
    }

    /// Canonical JSON (the `request_hash` input).
    pub fn to_json(&self) -> Json {
        match self {
            RetrievalQuery::ByAddress { address } => Json::obj([
                ("kind", Json::str("by_address")),
                ("address", Json::str(address.clone())),
            ]),
            RetrievalQuery::ByName { scope, name } => Json::obj([
                ("kind", Json::str("by_name")),
                ("scope", Json::str(scope.as_str())),
                ("name", Json::str(name.clone())),
            ]),
            RetrievalQuery::ByPathGlob { pattern } => Json::obj([
                ("kind", Json::str("by_path_glob")),
                ("pattern", Json::str(pattern.clone())),
            ]),
            RetrievalQuery::ByRun {
                run_id,
                seq_range,
                classes,
            } => {
                let mut v = vec![
                    ("kind", Json::str("by_run")),
                    ("run_id", Json::str(run_id.clone())),
                    (
                        "classes",
                        Json::Arr(classes.iter().map(|c| Json::str(c.clone())).collect()),
                    ),
                ];
                if let Some((lo, hi)) = seq_range {
                    v.push((
                        "seq_range",
                        Json::obj([
                            ("from_seq", Json::Int(*lo as i64)),
                            ("to_seq", Json::Int(*hi as i64)),
                        ]),
                    ));
                }
                Json::obj(v)
            }
            RetrievalQuery::Lexical {
                terms,
                match_mode,
                context_lines,
                case_sensitive,
                normalized,
            } => {
                let mm = match match_mode {
                    MatchMode::Any => Json::str("any"),
                    MatchMode::AllSameLine => Json::str("all_same_line"),
                    MatchMode::AllWithin { n } => Json::obj([("all_within", Json::Int(*n as i64))]),
                };
                Json::obj([
                    ("kind", Json::str("lexical")),
                    (
                        "terms",
                        Json::Arr(terms.iter().map(|t| Json::str(t.clone())).collect()),
                    ),
                    ("match_mode", mm),
                    ("context_lines", Json::Int(*context_lines as i64)),
                    ("case_sensitive", Json::Bool(*case_sensitive)),
                    ("normalized", Json::Bool(*normalized)),
                ])
            }
            RetrievalQuery::Discover {
                path,
                direction,
                filenames,
                ceiling,
            } => Json::obj([
                ("kind", Json::str("discover")),
                ("path", Json::str(path.clone())),
                (
                    "direction",
                    Json::str(match direction {
                        DiscoverDirection::Upward => "upward",
                        DiscoverDirection::JitDownward => "jit_downward",
                    }),
                ),
                (
                    "filenames",
                    Json::Arr(filenames.iter().map(|f| Json::str(f.clone())).collect()),
                ),
                (
                    "ceiling",
                    Json::str(match ceiling {
                        DiscoverCeiling::WorkspaceRoot => "workspace_root",
                        DiscoverCeiling::GitRoot => "git_root",
                    }),
                ),
            ]),
            RetrievalQuery::Trigger { kind } => {
                let (k, v) = match kind {
                    TriggerKind::Message(m) => ("message", m.clone()),
                    TriggerKind::PathTouched(p) => ("path_touched", p.clone()),
                    TriggerKind::Explicit(e) => ("explicit", e.clone()),
                };
                Json::obj([
                    ("kind", Json::str("trigger")),
                    ("trigger", Json::str(k)),
                    ("value", Json::str(v)),
                ])
            }
            RetrievalQuery::Structural {
                anchors,
                mentioned_idents,
            } => Json::obj([
                ("kind", Json::str("structural")),
                (
                    "anchors",
                    Json::Arr(anchors.iter().map(|a| Json::str(a.clone())).collect()),
                ),
                (
                    "mentioned_idents",
                    Json::Arr(
                        mentioned_idents
                            .iter()
                            .map(|i| Json::str(i.clone()))
                            .collect(),
                    ),
                ),
            ]),
            RetrievalQuery::Similarity { text, embedder } => Json::obj([
                ("kind", Json::str("similarity")),
                ("text", Json::str(text.clone())),
                ("embedder", Json::str(embedder.clone())),
                ("deterministic", Json::Bool(false)),
            ]),
        }
    }

    /// Strict decode — `BadMember`/`InvalidQueryKind`-shaped refusals at the
    /// codec boundary (the sum is closed).
    pub fn from_json(j: &Json) -> Result<RetrievalQuery, CodecError> {
        const REC: &str = "RetrievalQuery";
        let m = expect_obj(j, REC)?;
        let kind = str_at(m, "kind", REC)?;
        match kind {
            "by_address" => {
                reject_unknown(m, &["kind", "address"], REC)?;
                Ok(RetrievalQuery::ByAddress {
                    address: str_at(m, "address", REC)?.to_string(),
                })
            }
            "by_name" => {
                reject_unknown(m, &["kind", "scope", "name"], REC)?;
                let scope = PersistenceScope::parse(str_at(m, "scope", REC)?).ok_or_else(|| {
                    CodecError::TypeMismatch {
                        member: "scope".to_string(),
                        expected: "a PersistenceScope spelling",
                    }
                })?;
                Ok(RetrievalQuery::ByName {
                    scope,
                    name: str_at(m, "name", REC)?.to_string(),
                })
            }
            "by_path_glob" => {
                reject_unknown(m, &["kind", "pattern"], REC)?;
                Ok(RetrievalQuery::ByPathGlob {
                    pattern: str_at(m, "pattern", REC)?.to_string(),
                })
            }
            "by_run" => {
                reject_unknown(m, &["kind", "run_id", "seq_range", "classes"], REC)?;
                let seq_range = match m.get("seq_range") {
                    Some(Json::Obj(sr)) => {
                        reject_unknown(sr, &["from_seq", "to_seq"], REC)?;
                        Some((
                            int_at(sr, "from_seq", REC)? as u64,
                            int_at(sr, "to_seq", REC)? as u64,
                        ))
                    }
                    Some(Json::Null) | None => None,
                    Some(_) => {
                        return Err(CodecError::TypeMismatch {
                            member: "seq_range".to_string(),
                            expected: "object",
                        })
                    }
                };
                let classes = match m.get("classes") {
                    Some(Json::Arr(cs)) => cs
                        .iter()
                        .map(|c| {
                            c.as_str()
                                .map(str::to_string)
                                .ok_or_else(|| CodecError::TypeMismatch {
                                    member: "classes[]".to_string(),
                                    expected: "string",
                                })
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    Some(Json::Null) | None => Vec::new(),
                    Some(_) => {
                        return Err(CodecError::TypeMismatch {
                            member: "classes".to_string(),
                            expected: "array",
                        })
                    }
                };
                Ok(RetrievalQuery::ByRun {
                    run_id: str_at(m, "run_id", REC)?.to_string(),
                    seq_range,
                    classes,
                })
            }
            "lexical" => {
                reject_unknown(
                    m,
                    &[
                        "kind",
                        "terms",
                        "match_mode",
                        "context_lines",
                        "case_sensitive",
                        "normalized",
                    ],
                    REC,
                )?;
                let terms = arr_at(m, "terms", REC)?
                    .iter()
                    .map(|t| {
                        t.as_str()
                            .map(str::to_string)
                            .ok_or_else(|| CodecError::TypeMismatch {
                                member: "terms[]".to_string(),
                                expected: "string",
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let match_mode = match m.get("match_mode") {
                    Some(Json::Str(s)) if s.as_str() == "any" => MatchMode::Any,
                    Some(Json::Str(s)) if s.as_str() == "all_same_line" => MatchMode::AllSameLine,
                    Some(Json::Obj(mm)) => {
                        reject_unknown(mm, &["all_within"], REC)?;
                        MatchMode::AllWithin {
                            n: int_at(mm, "all_within", REC)? as u64,
                        }
                    }
                    _ => {
                        return Err(CodecError::TypeMismatch {
                            member: "match_mode".to_string(),
                            expected: "any | all_same_line | {all_within: n}",
                        })
                    }
                };
                Ok(RetrievalQuery::Lexical {
                    terms,
                    match_mode,
                    context_lines: int_at(m, "context_lines", REC)? as u64,
                    case_sensitive: crate::codec::bool_at(m, "case_sensitive", REC)?,
                    normalized: crate::codec::bool_at(m, "normalized", REC)?,
                })
            }
            "discover" => {
                reject_unknown(
                    m,
                    &["kind", "path", "direction", "filenames", "ceiling"],
                    REC,
                )?;
                let direction = match str_at(m, "direction", REC)? {
                    "upward" => DiscoverDirection::Upward,
                    "jit_downward" => DiscoverDirection::JitDownward,
                    _ => {
                        return Err(CodecError::TypeMismatch {
                            member: "direction".to_string(),
                            expected: "upward | jit_downward",
                        })
                    }
                };
                let ceiling = match str_at(m, "ceiling", REC)? {
                    "workspace_root" => DiscoverCeiling::WorkspaceRoot,
                    "git_root" => DiscoverCeiling::GitRoot,
                    _ => {
                        return Err(CodecError::TypeMismatch {
                            member: "ceiling".to_string(),
                            expected: "workspace_root | git_root",
                        })
                    }
                };
                Ok(RetrievalQuery::Discover {
                    path: str_at(m, "path", REC)?.to_string(),
                    direction,
                    filenames: str_set_at(m, "filenames", REC)?.into_iter().collect(),
                    ceiling,
                })
            }
            "trigger" => {
                reject_unknown(m, &["kind", "trigger", "value"], REC)?;
                let value = str_at(m, "value", REC)?.to_string();
                let kind = match str_at(m, "trigger", REC)? {
                    "message" => TriggerKind::Message(value),
                    "path_touched" => TriggerKind::PathTouched(value),
                    "explicit" => TriggerKind::Explicit(value),
                    _ => {
                        return Err(CodecError::TypeMismatch {
                            member: "trigger".to_string(),
                            expected: "message | path_touched | explicit",
                        })
                    }
                };
                Ok(RetrievalQuery::Trigger { kind })
            }
            "structural" => {
                reject_unknown(m, &["kind", "anchors", "mentioned_idents"], REC)?;
                Ok(RetrievalQuery::Structural {
                    anchors: str_set_at(m, "anchors", REC)?.into_iter().collect(),
                    mentioned_idents: str_set_at(m, "mentioned_idents", REC)?
                        .into_iter()
                        .collect(),
                })
            }
            "similarity" => {
                reject_unknown(m, &["kind", "text", "embedder", "deterministic"], REC)?;
                if let Some(Json::Bool(true)) = m.get("deterministic") {
                    // `similarity` declares deterministic = false — a `true`
                    // claim is a contract violation, never coerced.
                    return Err(CodecError::TypeMismatch {
                        member: "deterministic".to_string(),
                        expected: "false (similarity is non-deterministic)",
                    });
                }
                Ok(RetrievalQuery::Similarity {
                    text: str_at(m, "text", REC)?.to_string(),
                    embedder: str_at(m, "embedder", REC)?.to_string(),
                })
            }
            _ => Err(CodecError::TypeMismatch {
                member: "kind".to_string(),
                expected: "a registered RetrievalQuery kind",
            }),
        }
    }
}

/// The six reserved slot ids in kernel precedence order (§5c.1; ADR-0074 d1).
pub const RESERVED_SLOT_IDS: &[&str] = &[
    "kernel",
    "definition",
    "principal",
    "transcript",
    "external",
    "unverified",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_sums_round_trip() {
        for k in CANDIDATE_KINDS {
            assert_eq!(CandidateKind::parse(k.as_str()), Some(*k));
        }
        for p in PRIORITY_CLASSES {
            assert_eq!(PriorityClass::parse(p.as_str()), Some(*p));
        }
        for k in MEMORY_KINDS {
            assert_eq!(MemoryKind::parse(k.as_str()), Some(*k));
        }
        assert!(CandidateKind::parse("system").is_none()); // I-NOSYS: unrepresentable
        assert!(PriorityClass::parse("urgent").is_none());
        assert!(MemoryKind::parse("factoid").is_none());
    }

    #[test]
    fn priority_eviction_order_is_the_declaration_order() {
        // CF-176: memory_index ordered after memory.
        assert!(PriorityClass::Memory.eviction_rank() < PriorityClass::MemoryIndex.eviction_rank());
        assert!(
            PriorityClass::Commentary.eviction_rank()
                < PriorityClass::TranscriptTail.eviction_rank()
        );
    }

    #[test]
    fn retrieval_query_round_trips_strictly() {
        let q = RetrievalQuery::Lexical {
            terms: vec!["alpha".into(), "beta".into()],
            match_mode: MatchMode::AllWithin { n: 5 },
            context_lines: 2,
            case_sensitive: false,
            normalized: true,
        };
        assert_eq!(RetrievalQuery::from_json(&q.to_json()).unwrap(), q);
        let mut m = match q.to_json() {
            Json::Obj(m) => m,
            _ => unreachable!(),
        };
        m.insert("bogus".into(), Json::Null);
        assert!(matches!(
            RetrievalQuery::from_json(&Json::Obj(m)),
            Err(CodecError::BadMember { .. })
        ));
        // An unregistered kind is a typed refusal, never a silent default.
        let bad = Json::obj([("kind", Json::str("semantic_search"))]);
        assert!(RetrievalQuery::from_json(&bad).is_err());
    }
}
