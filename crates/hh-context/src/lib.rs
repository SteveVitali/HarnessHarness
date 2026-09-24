//! `hh-context` — the C0/Stage-1 context builder / policy engine and the C0
//! memory slices (spec §5c.1 R-2.4.1, §5c.3 `R-2.4.3⁰`, §5c.4 `R-2.4.4⁰`;
//! ticket S1.19; ADR-0072…0083).
//!
//! ## The context builder (§5c.1)
//!
//! [`assemble::assemble`] is the kernel's fixed step order —
//! `admit` (I-ID, I-RP, I-ORDER, I-MEDIA, I-NOSYS) → `reserve_mandatory` →
//! `policy.select` → `enforce_budget` → `check_invariants` → `plan` — producing
//! the content-addressed [`plan::ContextPlan`]. The six kernel-reserved slots
//! with fixed floors (`kernel`/`definition`/`principal`/`transcript`/`external`/
//! `unverified`) are [`plan::RESERVED_SLOTS`]; a layout missing one or lowering a
//! floor fails `link` with [`plan::LayoutError::InvalidLayout`], a `static` slot
//! admitting a volatile kind with [`plan::LayoutError::VolatileInStaticTier`]
//! (AC-R-2.4.1-14). The selection policy is a registered component variant
//! ([`policy::ContextPolicy`]) that chooses membership/slot/order among
//! kernel-admitted candidates and can never widen — the kernel re-checks every
//! `Selection` and a widening attempt is
//! [`assemble::AssemblyError::PolicyViolation`] with no plan emitted
//! (AC-R-2.4.1-4). `ContextWindowExceeded` is an error, never a truncation;
//! at Stage 0/1 [`assemble::stage01_disposition`] maps it to
//! `CompactionRequired`-as-stop until `evict_oldest` exists (R-2.4.2).
//! [`events::assembled_payload`] renders the `context.assembled` ledger row
//! (the M5 measurement point's producer — `assembly_ms` stamped
//! `MeasuredAt::Runtime`, DF-S1.14-1).
//!
//! ## The C0 memory slices (§5c.3/§5c.4)
//!
//! [`memory::MemoryStore`] is the runtime-owned store: `put`/`bind`/`revoke`/
//! `resolve`/`manifest`/`enumerate`/`stale_candidates` over immutable
//! [`memory::MemoryVersion`]s with per-scope writer leases (fenced writes
//! refused), the `delegate` write ceiling and the `external` free-text cap
//! (R-TEXT). [`lifecycle`] carries the `R-2.4.4⁰` pure functions —
//! `lifecycle_state`, `check_contract`, `filter_for_slot`, `write` semantics
//! (E1–E4), deterministic conflict sets, `promote`/`revalidate` and the
//! `MemoryStaleIndex` fold. [`retrieve`] is the one retrieval pipeline —
//! `enumerate` → validity → authority → readers → `deterministic_default`
//! ranker → budgeted whole-item cut — with `lexical_index` as the C0
//! materialized view (stamped through `hh-ledger`'s `View` — one view shape,
//! CC7) and every retrieval ledgered via `context.retrieval.completed` +
//! `context.memory.read` payloads.
//!
//! Records this crate does **not** redefine (CC7): `Label`/`ProvenanceRecord`/
//! `AuthorityClass`/`PersistenceScope` (`hh-provenance`), `VersionedRef`/
//! `Lineage`/`RevocationRecord`/`idp` (`hh-identity`), `Validity`/`Text`/
//! `ArtifactRecord` (`hh-hir`), `Event`/`View` (`hh-ledger`),
//! `MeasuredAt` (`hh-telemetry`).

pub mod assemble;
pub mod codec;
pub mod events;
pub mod lifecycle;
pub mod memory;
pub mod plan;
pub mod policy;
pub mod retrieve;
pub mod vocab;

pub use assemble::{
    assemble, stage01_disposition, AssemblyError, AssemblyOutcome, AssemblyRequest,
    CompactionRequired, Stage01Outcome, OMISSION_ITEM_TOKENS,
};
pub use codec::CodecError;
pub use events::{CollectSink, EventSink};
pub use lifecycle::{
    check_contract, check_revoke_authority, check_supersede_authority, dependants, filter_for_slot,
    item_lifecycle_state, lifecycle_state, memory_usage, promote, resolve_conflict, revalidate,
    revoke, stale_index, ContractCheck, FilterItem, FilterOutcome, LifecycleEnv, LifecycleError,
    MemoryStaleIndex, MemoryUsageEntry, Withheld,
};
pub use memory::{
    ArtifactVersion, ConflictSet, DependencyStamp, Freshness, InvalidationContract, Justification,
    JustificationKind, KindPolicy, MemoryDraft, MemoryError, MemoryManifest, MemoryPolicy,
    MemoryRevocation, MemoryStore, MemoryVersion, NameBinding, PutOutcome, ResolveOutcome,
    SupersedeClaim, SupersedeClaimReason, WriteContext,
};
pub use plan::{
    default_layout, excerpt, link_layout, offload, Candidate, Cardinality, ContextBudget,
    ContextPlan, CutPoint, DerivedFrom, Estimate, ExcerptMode, ExcerptPolicy, ExcerptReport,
    Layout, LayoutError, OffloadError, OffloadHandle, Omission, PlannedItem, Reservation,
    SlotDeclaration, SlotFill, SlotOrder, ValidityPolicy, RESERVED_SLOTS,
};
pub use policy::{
    check as check_policy, check_selection, AdmittedCandidate, ConditionedRule, ContextPolicy,
    DefaultPolicy, PolicyDeclaration, PolicyRequest, PolicyViolation, RegistrationError,
    RuleCondition, Selection, REQUIRED_POLICY_INPUTS,
};
pub use retrieve::{
    artifact_candidate, as_candidate, lexical_index, rank_score, retrieve, tokenize, LexicalIndex,
    RankEvidence, RetrievalBudget, RetrievalCost, RetrievalError, RetrievalReport,
    RetrievalRequest, RetrievedItem, SlotConstraints, DETERMINISTIC_DEFAULT,
};
pub use vocab::{
    CacheHint, CacheTier, CandidateKind, CandidateState, ConflictPolicy, ConflictResolution,
    DependencyKind, DiscoverCeiling, DiscoverDirection, Granularity, InvalidationCondition, Layer,
    LifecycleState, LifecycleStateKind, MatchMode, MemoryContent, MemoryKind, OmissionReason,
    PriorityClass, Retention, RetrievalQuery, Revalidation, RevocationReason, SubjectKey,
    TriggerKind, CANDIDATE_KINDS, MEMORY_KINDS, PRIORITY_CLASSES, RESERVED_SLOT_IDS,
    VOLATILE_KINDS,
};
