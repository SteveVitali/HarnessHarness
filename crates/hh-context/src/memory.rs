//! `R-2.4.3⁰` — the C0 `MemoryStore` (§5c.3; ADR-0077…0080): writes are
//! immutable [`MemoryVersion`]s, never in-place mutation; `bind`/`manifest`
//! are name-history bindings (`NameBindingRecord`-shaped, version-only);
//! `revoke` is hash-chained through `hh-identity`'s `Lineage`; `resolve` under
//! `ResolveMode::Execute` never serves a revoked/stale/superseded head;
//! per-scope writer leases fence every write; the `delegate` write ceiling
//! and `external` free-text cap apply to delegate-class writers (R-TEXT).
//!
//! The store is runtime-owned infrastructure: callers hand `put`/`bind`/
//! `revoke` a [`WriteContext`] carrying `context_label` + `lease_generation`
//! and the store emits ledger payload rows ([`crate::events`]) for the caller
//! to append — the store itself never touches `hh-ledger`'s `Store` (the
//! kernel owns append; the payloads are the contract).
//!
//! `enumerate`/`retrieve`/`lexical_index`/`stale_index`/`memory_usage` are
//! materialized views — derived, rebuild-equal under the same append order
//! (§5c.3; [`crate::retrieve`]).

use std::collections::{BTreeMap, BTreeSet};

use hh_hir::records::Validity;
use hh_identity::idp::{identify_bytes, idp_id};
use hh_identity::kinds::RecordKind;
use hh_identity::refs::VersionedRef;
use hh_identity::supersede::{Lineage, SupersedeReason, SupersedesEdge};
use hh_provenance::label::Label;
use hh_provenance::origin::Origin;
use hh_provenance::record::ProvenanceRecord;
use hh_provenance::PersistenceScope;
use hh_wire::json::Json;

use crate::codec::label_json;
use crate::events;
use crate::vocab::{
    CacheHint, ConflictResolution, DependencyKind, Granularity, InvalidationCondition, Layer,
    LifecycleStateKind, MemoryContent, MemoryKind, Revalidation, RevocationReason, SubjectKey,
};

/// `memory_line.1` — the `idp` domain for fresh `semantic_id`s (a memory's
/// supersession line; §5c.3).
pub const MEMORY_LINE_IDP: &str = "memory_line.1";

/// `memory_conflict.1` — the `idp` domain for `ConflictSet` ids.
pub const CONFLICT_IDP: &str = "memory_conflict.1";

// ─────────────────────────────────────────────────────────────────────────────
// Records (§5c.3 data model)
// ─────────────────────────────────────────────────────────────────────────────

/// `DependencyStamp{kind, ref, stamp, granularity}` (§5c.4): `ref` names a
/// versioned kernel/registry record (a `RecordId`/`VersionedRef` — never a
/// display name); `stamp` is the version id / revision / snapshot hash the
/// write pinned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyStamp {
    /// The closed kind.
    pub kind: DependencyKind,
    /// The pinned `VersionedRef`/record id.
    pub ref_: String,
    /// The pinned stamp.
    pub stamp: String,
    /// `row | table`.
    pub granularity: Granularity,
}

/// `Freshness` — `valid_until(at)` | `max_age(n, from)` (§5c.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Freshness {
    /// Valid through the absolute logical `at`.
    ValidUntil(u64),
    /// Valid for `max_age` units after `from`.
    MaxAge {
        /// The bound.
        max_age: u64,
        /// The origin instant.
        from: u64,
    },
}

impl Freshness {
    /// Whether the freshness holds at `at` (`None` ⇒ elapsed).
    pub fn holds_at(&self, at: u64) -> bool {
        match self {
            Freshness::ValidUntil(u) => at < *u,
            Freshness::MaxAge { max_age, from } => at < from.saturating_add(*max_age),
        }
    }
}

/// `InvalidationContract` — the write-time record of what would invalidate
/// the memory (§5c.4; ADR-0081):
/// `{dependencies[], cache_hint, validator_ref?, freshness?, invalidation_condition?,
///  revalidation}`. At least one checkable member is required for scope
/// `session|project|user` (C-CONTRACT-1; `run`/`turn` may carry the empty
/// contract — end-of-run is the invalidation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidationContract {
    /// `dependencies[]` — `DependencyStamp`s.
    pub dependencies: Vec<DependencyStamp>,
    /// `cache_hint`.
    pub cache_hint: CacheHint,
    /// `validator_ref` — `Ref` to a `KernelValidator`.
    pub validator_ref: Option<String>,
    /// `freshness`.
    pub freshness: Option<Freshness>,
    /// `invalidation_condition` — one closed condition.
    pub invalidation_condition: Option<InvalidationCondition>,
    /// `revalidation`.
    pub revalidation: Revalidation,
}

impl InvalidationContract {
    /// `true` when nothing is checkable (no dependencies, no validator, no
    /// freshness, no condition) — C-CONTRACT-1 refuses this at `session|project|
    /// user` scope.
    pub fn is_empty(&self) -> bool {
        self.dependencies.is_empty()
            && self.validator_ref.is_none()
            && self.freshness.is_none()
            && self.invalidation_condition.is_none()
    }
}

/// `JustificationKind` — what a `Justification` names (§5c.4; ADR-0082 d6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustificationKind {
    /// A delivered memory.
    DeliveredMemory,
    /// An expanded handle.
    ExpandedHandle,
    /// A declared input (`declared_inputs` member — excluded from the
    /// stale-by-dependency read per J1).
    DeclaredInput,
    /// A dependency stamp.
    DependencyStamp,
}

impl JustificationKind {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            JustificationKind::DeliveredMemory => "delivered_memory",
            JustificationKind::ExpandedHandle => "expanded_handle",
            JustificationKind::DeclaredInput => "declared_input",
            JustificationKind::DependencyStamp => "dependency_stamp",
        }
    }
}

/// `Justification{kind, ref: VersionedRef, at: EventRef}` (§5c.3/§5c.4).
/// Carried on every `MemoryVersion` — the memory's provenance of *content*
/// (J1/J3 read this; `declared_input`-kinded members are excluded from the
/// stale-by-dependency read).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Justification {
    /// The closed kind.
    pub kind: JustificationKind,
    /// The `VersionedRef` this write rested on.
    pub ref_: VersionedRef,
    /// The `EventRef` of the delivery/expand/read (`run_id:event_id`).
    pub at: hh_ledger::manifest::EventRef,
}

/// `SupersedeClaim` — a `put`'s declared supersession intent (§5c.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupersedeClaim {
    /// The version superseded.
    pub version_id: String,
    /// The closed reason.
    pub reason: SupersedeClaimReason,
}

/// `SupersedeClaimReason` — `correction | replacement | compaction |
/// migration | consolidation` (§5c.3; the write-side reasons — the
/// lineage-side `SupersedeReason` is `hh-identity`'s).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupersedeClaimReason {
    /// A correction.
    Correction,
    /// A replacement.
    Replacement,
    /// A compaction output.
    Compaction,
    /// A migration.
    Migration,
    /// A consolidation output.
    Consolidation,
}

impl SupersedeClaimReason {
    /// The canonical spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SupersedeClaimReason::Correction => "correction",
            SupersedeClaimReason::Replacement => "replacement",
            SupersedeClaimReason::Compaction => "compaction",
            SupersedeClaimReason::Migration => "migration",
            SupersedeClaimReason::Consolidation => "consolidation",
        }
    }

    /// The lineage `SupersedeReason` this claim maps to.
    pub fn lineage_reason(self) -> SupersedeReason {
        match self {
            SupersedeClaimReason::Correction | SupersedeClaimReason::Replacement => {
                SupersedeReason::Edit
            }
            SupersedeClaimReason::Compaction | SupersedeClaimReason::Consolidation => {
                SupersedeReason::Consolidation
            }
            SupersedeClaimReason::Migration => SupersedeReason::Migration,
        }
    }
}

/// `MemoryVersion` — one immutable write (§5c.3 data model; §5c.4 reads
/// `v.contract`, `v.scope`, `v.justifications`, `v.declared_inputs`,
/// `v.supersedes_claim`, `v.validity`).
#[derive(Debug, Clone)]
pub struct MemoryVersion {
    /// `version_id` — the idp content address (`RecordKind::Memory`).
    pub version_id: String,
    /// `semantic_id` — the supersession line identity (same line = same
    /// `semantic_id`; minted at first write, carried on supersessions).
    pub semantic_id: String,
    /// `kind` — the closed `MemoryKind`.
    pub kind: MemoryKind,
    /// `subject_key` — the deterministic-conflict key.
    pub subject_key: Option<SubjectKey>,
    /// `content` — `Text | StructuredValue`.
    pub content: MemoryContent,
    /// `contract` — the invalidation contract (post policy-default merge).
    pub contract: InvalidationContract,
    /// `scope` — where the version lives.
    pub scope: PersistenceScope,
    /// `label` — `join(context_label, writer_label)` capped by the write
    /// ceiling / R-TEXT (§5c.3 write path).
    pub label: Label,
    /// `provenance` — the minted write record.
    pub provenance: ProvenanceRecord,
    /// `validity` — the declared window (§5c.4 reads `v.validity`).
    pub validity: Option<Validity>,
    /// `declared_inputs[]` — the declared-input refs (excluded from J1's
    /// stale read).
    pub declared_inputs: Vec<VersionedRef>,
    /// `justifications[]`.
    pub justifications: Vec<Justification>,
    /// `created_at` — the write seq.
    pub created_at: u64,
    /// `created_by` — the writer's rendered origin.
    pub created_by: String,
    /// `supersedes_claim` — the declared supersession (applied at write).
    pub supersedes_claim: Option<SupersedeClaim>,
    /// `conflict_set_ref` — the deterministic conflict set this version
    /// belongs to (set at write, updated on `resolve_conflict`).
    pub conflict_set_ref: Option<String>,
    /// `validator_endorsed` — a `kernel`-validator endorsement on a
    /// `StructuredValue` (raises the R-TEXT cap to `environment`).
    pub validator_endorsed: bool,
}

impl MemoryVersion {
    /// The canonical body minus `version_id` — the `identify_bytes` preimage.
    pub fn body_json(&self) -> Json {
        let mut v = vec![
            ("semantic_id", Json::str(self.semantic_id.clone())),
            ("kind", Json::str(self.kind.as_str())),
            ("scope", Json::str(self.scope.as_str())),
            ("label", label_json(&self.label)),
            ("provenance", self.provenance.to_json()),
            (
                "content",
                match &self.content {
                    MemoryContent::Text(t) => {
                        Json::obj([("text", Json::str(t.content.clone().unwrap_or_default()))])
                    }
                    MemoryContent::Structured(j) => Json::obj([("structured", j.clone())]),
                },
            ),
            ("contract", contract_json(&self.contract)),
            ("created_at", Json::Int(self.created_at as i64)),
            ("created_by", Json::str(self.created_by.clone())),
            (
                "declared_inputs",
                Json::Arr(self.declared_inputs.iter().map(vref_json_str).collect()),
            ),
            (
                "justifications",
                Json::Arr(
                    self.justifications
                        .iter()
                        .map(|j| {
                            Json::obj([
                                ("kind", Json::str(j.kind.as_str())),
                                ("ref", Json::str(vref_str(&j.ref_))),
                                ("at", Json::str(crate::codec::event_ref_str(&j.at))),
                            ])
                        })
                        .collect(),
                ),
            ),
        ];
        if let Some(sk) = &self.subject_key {
            v.push((
                "subject_key",
                Json::obj([
                    ("schema_ref", Json::str(sk.schema_ref.clone())),
                    ("key", Json::str(sk.key.clone())),
                ]),
            ));
        }
        if let Some(vd) = &self.validity {
            v.push(("validity", crate::codec::validity_json(vd)));
        }
        if let Some(sc) = &self.supersedes_claim {
            v.push((
                "supersedes_claim",
                Json::obj([
                    ("version_id", Json::str(sc.version_id.clone())),
                    ("reason", Json::str(sc.reason.as_str())),
                ]),
            ));
        }
        if self.validator_endorsed {
            v.push(("validator_endorsed", Json::Bool(true)));
        }
        Json::obj(v)
    }
}

fn contract_json(c: &InvalidationContract) -> Json {
    let mut v = vec![
        (
            "dependencies",
            Json::Arr(
                c.dependencies
                    .iter()
                    .map(|d| {
                        Json::obj([
                            ("kind", Json::str(d.kind.as_str())),
                            ("ref", Json::str(d.ref_.clone())),
                            ("stamp", Json::str(d.stamp.clone())),
                            ("granularity", Json::str(d.granularity.as_str())),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("cache_hint", Json::str(c.cache_hint.as_str())),
        (
            "revalidation",
            match c.revalidation {
                Revalidation::MustRevalidate => Json::str("must_revalidate"),
                Revalidation::StaleOk { max_stale } => Json::obj([(
                    "stale_ok",
                    Json::obj([("max_stale", Json::Int(max_stale as i64))]),
                )]),
                Revalidation::Never => Json::str("never"),
            },
        ),
    ];
    if let Some(vr) = &c.validator_ref {
        v.push(("validator_ref", Json::str(vr.clone())));
    }
    if let Some(f) = &c.freshness {
        v.push((
            "freshness",
            match f {
                Freshness::ValidUntil(u) => Json::obj([("valid_until", Json::Int(*u as i64))]),
                Freshness::MaxAge { max_age, from } => Json::obj([(
                    "max_age",
                    Json::obj([
                        ("n", Json::Int(*max_age as i64)),
                        ("from", Json::Int(*from as i64)),
                    ]),
                )]),
            },
        ));
    }
    if let Some(ic) = &c.invalidation_condition {
        let mut icv = vec![("kind", Json::str(ic.kind()))];
        match ic {
            InvalidationCondition::ScopeEnded(s) => {
                icv.push(("scope", Json::str(s.as_str())));
            }
            InvalidationCondition::ReplacementPublished(n) => {
                icv.push(("name", Json::str(n.clone())));
            }
            _ => {}
        }
        v.push(("invalidation_condition", Json::obj(icv)));
    }
    Json::obj(v)
}

/// `Artifact` — the layer-A record (§5c.3): content-addressed, file-shaped.
/// The `content_address` is the `hh-ledger` blob id / `context_offload` id.
#[derive(Debug, Clone)]
pub struct ArtifactVersion {
    /// `artifact_id` — the content address.
    pub artifact_id: String,
    /// The logical path (a `path_glob`/`discover` key).
    pub path: String,
    /// `created_at` seq.
    pub created_at: u64,
    /// The artifact's label.
    pub label: Label,
    /// The derived-from run (a `by_run` key).
    pub run_id: String,
    /// The event classes this artifact was derived from (a `by_run.classes`
    /// key).
    pub classes: BTreeSet<String>,
    /// The pinned estimator's token estimate.
    pub tokens: u64,
    /// The searchable body (excerpt/metadata — the full body lives behind the
    /// content address; this is the C0 index surface).
    pub index_text: String,
}

/// `NameBinding{scope, name, version_id, bound_at, supersedes?, reason}` —
/// one entry of the name history (`bind` appends, never edits).
#[derive(Debug, Clone)]
pub struct NameBinding {
    /// The scope.
    pub scope: PersistenceScope,
    /// The name.
    pub name: String,
    /// The bound version.
    pub version_id: String,
    /// The binding seq.
    pub bound_at: u64,
    /// The prior binding this supersedes.
    pub supersedes: Option<String>,
    /// The binding reason.
    pub reason: String,
}

/// `MemoryManifest{scope, entries[]}` — the folded name history at `at`
/// (§5c.3). Content-addressed (`RecordKind::MemoryManifest`).
#[derive(Debug, Clone)]
pub struct MemoryManifest {
    /// The manifest's content address.
    pub manifest_id: String,
    /// The scope.
    pub scope: PersistenceScope,
    /// `name → current bound version` (sorted by name — CC4 determinism).
    pub entries: BTreeMap<String, String>,
    /// The seq the manifest answers.
    pub at_seq: u64,
}

/// `ConflictSet` — the deterministic conflict record (§5c.4; ADR-0082 d3):
/// `{conflict_set_id, subject_key, members[], detector, resolution,
/// escalated_to?}`.
#[derive(Debug, Clone)]
pub struct ConflictSet {
    /// `conflict_set_id` — content-addressed (`memory_conflict.1`).
    pub conflict_set_id: String,
    /// The shared `SubjectKey`.
    pub subject_key: SubjectKey,
    /// The member version ids (sorted — deterministic).
    pub members: Vec<String>,
    /// `detector` — `deterministic` at C0 (the `judged` tail is C2).
    pub detector: String,
    /// The resolution.
    pub resolution: ConflictResolution,
    /// `escalated_to` — the principal ref when escalated.
    pub escalated_to: Option<String>,
}

/// A memory revocation record — the `context.memory.invalidated` payload's
/// record side (the hash-chained `RevocationRecord` lives in `Lineage`).
#[derive(Debug, Clone)]
pub struct MemoryRevocation {
    /// The revoked version.
    pub version_id: String,
    /// The closed reason.
    pub reason: RevocationReason,
    /// The revoker's provenance.
    pub revoker: ProvenanceRecord,
    /// The replacement version, when named.
    pub replacement: Option<String>,
    /// The revocation seq.
    pub at_seq: u64,
}

/// `MemoryPolicy` — the runtime record holding write ceilings, per-kind
/// defaults and escalation rules (§5c.3/§5c.4; ADR-0078 d4).
#[derive(Debug, Clone)]
pub struct MemoryPolicy {
    /// The write ceiling for delegate-class writers (`delegate` per §5c.3).
    pub write_ceiling: hh_provenance::AuthorityClass,
    /// `kind → {default_contract, allowed_scopes, granularity_allowed,
    /// min_authority_for_storage}` — the per-kind write defaults the ticket
    /// makes *declared data* (never silent defaults — a kind with no entry
    /// gets the conservative default: `run|turn` scopes only, row
    /// granularity, empty default contract ⇒ `session|project|user` writes
    /// need an explicit contract — C-CONTRACT-1).
    pub per_kind_defaults: BTreeMap<MemoryKind, KindPolicy>,
    /// Whether an unresolved conflict set escalates (§5c.4).
    pub escalate_on_unresolved: bool,
    /// The principal ref escalation targets.
    pub escalation_principal: String,
}

/// `KindPolicy` — the per-kind defaults record.
#[derive(Debug, Clone)]
pub struct KindPolicy {
    /// The contract merged into a draft's (when the draft's is empty).
    pub default_contract: Option<InvalidationContract>,
    /// The scopes the kind may write to.
    pub allowed_scopes: BTreeSet<PersistenceScope>,
    /// Whether `table`-granularity stamps are legal for the kind.
    pub granularity_allowed: bool,
    /// The minimum authority a stored version may carry for this kind.
    pub min_authority_for_storage: hh_provenance::AuthorityClass,
}

impl Default for MemoryPolicy {
    fn default() -> Self {
        MemoryPolicy {
            write_ceiling: hh_provenance::AuthorityClass::Delegate,
            per_kind_defaults: BTreeMap::new(),
            escalate_on_unresolved: true,
            escalation_principal: "principal".to_string(),
        }
    }
}

/// `WriteContext` — what a write carries (§5c.3 `put`/`write` signature):
/// `context_label` (the write-time join input), `lease_generation` (the
/// scope's writer lease — fenced writes refuse), the write's logical seq and
/// run.
#[derive(Debug, Clone)]
pub struct WriteContext {
    /// The assembled-context label the write's join reads.
    pub context_label: Label,
    /// The writer's lease generation for `scope`.
    pub lease_generation: u64,
    /// The write seq.
    pub at_seq: u64,
    /// The run the write belongs to.
    pub run_id: String,
}

/// `MemoryDraft` — the `put` input (a draft, never a stored record).
#[derive(Debug, Clone)]
pub struct MemoryDraft {
    /// `kind`.
    pub kind: MemoryKind,
    /// `subject_key` — the deterministic-conflict key.
    pub subject_key: Option<SubjectKey>,
    /// `content`.
    pub content: MemoryContent,
    /// `contract` — merged with `MemoryPolicy.per_kind_defaults` at write.
    pub contract: Option<InvalidationContract>,
    /// `scope`.
    pub scope: PersistenceScope,
    /// `declared_inputs[]` — `VersionedRef`s.
    pub declared_inputs: Vec<VersionedRef>,
    /// `justifications[]` — may be empty at the call; the kernel populates.
    pub justifications: Vec<Justification>,
    /// `supersedes` — the declared claim.
    pub supersedes: Option<SupersedeClaim>,
    /// `validity` — the declared window.
    pub validity: Option<Validity>,
    /// The writer's provenance record (mandatory — `MissingProvenance`
    /// refuses).
    pub provenance: Option<ProvenanceRecord>,
    /// `semantic_id` — join an existing version line; `None` mints fresh.
    pub semantic_id: Option<String>,
    /// `validator_endorsed` — a `kernel` validator endorsed a
    /// `StructuredValue` (the `environment` cap).
    pub validator_endorsed: bool,
}

/// `put`'s outcome — the stored record plus the `context.memory.written`
/// payload (the caller appends it through its own `EventSink`/ledger).
#[derive(Debug, Clone)]
pub struct PutOutcome {
    /// The stored version.
    pub version: MemoryVersion,
    /// The `context.memory.written` payload row.
    pub event: (String, Json),
    /// The conflict set created/extended, if any.
    pub conflict: Option<ConflictSet>,
}

/// `ResolveOutcome` — `resolve` under `mode` (§5c.3): `Execute` refuses
/// revoked/stale/superseded heads; `Audit` returns the record annotated.
#[derive(Debug, Clone)]
pub enum ResolveOutcome {
    /// The live head.
    Live {
        /// The version.
        version: MemoryVersion,
    },
    /// The head exists but is not servable in execute mode (audit returns it
    /// via `Annotated` instead).
    Unservable {
        /// The head version id.
        version_id: String,
        /// The lifecycle state kind.
        state: LifecycleStateKind,
    },
    /// Audit mode: the record plus its state annotation.
    Annotated {
        /// The version.
        version: MemoryVersion,
        /// The lifecycle state kind at the resolve seq.
        state: LifecycleStateKind,
    },
}

/// The `MemoryStore` failures (§5c.3/§5c.4 error lists).
#[derive(Debug, Clone, PartialEq)]
pub enum MemoryError {
    /// `MissingProvenance` — the write carried no provenance record.
    MissingProvenance,
    /// `MissingContract` — nothing checkable and no policy default at a
    /// `session|project|user` scope (C-CONTRACT-1).
    MissingContract {
        /// The scope.
        scope: PersistenceScope,
    },
    /// `ScopeCeilingExceeded` — a write to a scope above its class's ceiling
    /// (`definition` needs `seal`; `user` needs `approval`).
    ScopeCeilingExceeded {
        /// The scope.
        scope: PersistenceScope,
        /// The writer class.
        writer: String,
    },
    /// `Fenced` — the writer's lease generation is stale.
    Fenced {
        /// The scope.
        scope: PersistenceScope,
        /// The store's current generation.
        expected: u64,
        /// The generation the write carried.
        got: u64,
    },
    /// `UnknownVersion` — a `supersedes`/`resolve`/`revoke` target.
    UnknownVersion {
        /// The id.
        version_id: String,
    },
    /// `UnknownName` — an unbound `resolve` selector.
    UnknownName {
        /// The scope.
        scope: PersistenceScope,
        /// The name.
        name: String,
    },
    /// `CycleDetected` — a supersession edge would close a cycle.
    CycleDetected {
        /// The newer.
        newer: String,
        /// The older.
        older: String,
    },
    /// `KindMismatch` — a supersession across kinds.
    KindMismatch {
        /// The newer kind.
        newer: MemoryKind,
        /// The older kind.
        older: MemoryKind,
    },
    /// `AuthorityInsufficient` — the supersession's authority < the
    /// superseded's (the deterministic conflict path takes over).
    AuthorityInsufficient {
        /// The new authority.
        new_authority: String,
        /// The superseded's.
        old_authority: String,
    },
    /// `TaintedAboveExternal` — provenance validation refused the label.
    TaintedAboveExternal {
        /// Detail.
        detail: String,
    },
    /// `NotPersistable` — `no_store`/`recompute`-past-turn content refuses
    /// persistence (§5c.4 `cache_hint` semantics).
    NotPersistable {
        /// Why.
        detail: String,
    },
    /// `OriginOriginForbidden` — an `origin(origin_k)` ref reached the store.
    OriginForbidden {
        /// Detail.
        detail: String,
    },
}

impl std::fmt::Display for MemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MemoryError {}

// ─────────────────────────────────────────────────────────────────────────────
// The store
// ─────────────────────────────────────────────────────────────────────────────

/// `MemoryStore` — the C0 store (§5c.3; ADR-0077). All writes are immutable
/// `MemoryVersion`s; the `Lineage` machine owns supersession edges,
/// revocations and the direct dependant index; `stale_by_dependency`
/// transitivity (J1) is computed in `lifecycle_state`.
///
/// The store records reads (`record_read`) so `memory_usage` is rebuild-equal.
pub struct MemoryStore {
    /// The store id (the `run_id` slot of store-level view stamps).
    pub store_id: String,
    /// The write policy.
    pub policy: MemoryPolicy,
    /// `version_id → record` (immutable).
    versions: BTreeMap<String, MemoryVersion>,
    /// Write order — `version_id`s in applied seq order (the fold order).
    order: Vec<String>,
    /// `"scope:name" → binding history`.
    names: BTreeMap<String, Vec<NameBinding>>,
    /// The shared lineage machine (`hh-identity`'s — supersession edges,
    /// revocation records, direct stale index).
    lineage: Lineage,
    /// Memory-reason revocations (the `RevocationReason`-precise rows — the
    /// `Lineage` carries the coarse `SupersedeReason` for the graph).
    revocations: Vec<MemoryRevocation>,
    /// `scope → lease generation`.
    leases: BTreeMap<PersistenceScope, u64>,
    /// `scope → lease holder`.
    lease_holders: BTreeMap<PersistenceScope, String>,
    /// `conflict_set_id → set`.
    conflicts: BTreeMap<String, ConflictSet>,
    /// `version_id → read seqs` (the `memory_usage` fold input).
    reads: BTreeMap<String, Vec<u64>>,
    /// `artifact_id → artifact` (layer A).
    artifacts: BTreeMap<String, ArtifactVersion>,
    /// The store's applied watermark (retrieval `at ≤ applied_seq` — R3).
    applied_seq: u64,
    /// Emitted ledger payload rows `(class, payload)` — drained by the caller.
    emitted: Vec<(String, Json)>,
    /// `dep ref → current stamp` — the `check_contract` environment (kernel/
    /// registry records update the current stamp through `set_stamp`).
    stamps: BTreeMap<String, String>,
    /// `(version_id, validator_ref) → last verdict` — the validator port.
    verdicts: BTreeMap<(String, String), bool>,
    /// Ended scopes (`invalidation_condition: scope_ended` evaluation).
    ended_scopes: BTreeSet<PersistenceScope>,
    /// Names with a published replacement (`replacement_published`).
    published_replacements: BTreeSet<String>,
}

impl Default for MemoryStore {
    fn default() -> Self {
        MemoryStore::new("memory_store")
    }
}

impl MemoryStore {
    /// A store with the default `MemoryPolicy`.
    pub fn new(store_id: impl Into<String>) -> MemoryStore {
        MemoryStore {
            store_id: store_id.into(),
            policy: MemoryPolicy::default(),
            versions: BTreeMap::new(),
            order: Vec::new(),
            names: BTreeMap::new(),
            lineage: Lineage::new(),
            revocations: Vec::new(),
            leases: BTreeMap::new(),
            lease_holders: BTreeMap::new(),
            conflicts: BTreeMap::new(),
            reads: BTreeMap::new(),
            artifacts: BTreeMap::new(),
            applied_seq: 0,
            emitted: Vec::new(),
            stamps: BTreeMap::new(),
            verdicts: BTreeMap::new(),
            ended_scopes: BTreeSet::new(),
            published_replacements: BTreeSet::new(),
        }
    }

    /// `take_lease(scope, holder)` → generation — the write-lease op (§5c.3).
    pub fn take_lease(&mut self, scope: PersistenceScope, holder: &str) -> u64 {
        let g = self.leases.get(&scope).copied().unwrap_or(0) + 1;
        self.leases.insert(scope, g);
        self.lease_holders.insert(scope, holder.to_string());
        g
    }

    /// The current lease generation for `scope` (`0` = never taken).
    pub fn lease(&self, scope: PersistenceScope) -> u64 {
        self.leases.get(&scope).copied().unwrap_or(0)
    }

    /// The store's applied watermark.
    pub fn applied_seq(&self) -> u64 {
        self.applied_seq
    }

    /// Drain the emitted `(class, payload)` rows.
    pub fn drain_events(&mut self) -> Vec<(String, Json)> {
        std::mem::take(&mut self.emitted)
    }

    /// The stored version.
    pub fn version(&self, version_id: &str) -> Option<&MemoryVersion> {
        self.versions.get(version_id)
    }

    /// All version ids in write order (the fold order).
    pub fn version_order(&self) -> &[String] {
        &self.order
    }

    /// The lineage edges (read-only — the fold input).
    pub fn edges(&self) -> &[SupersedesEdge] {
        self.lineage.edges()
    }

    /// The memory revocations (read-only).
    pub fn revocations(&self) -> &[MemoryRevocation] {
        &self.revocations
    }

    /// The conflict sets.
    pub fn conflicts(&self) -> &BTreeMap<String, ConflictSet> {
        &self.conflicts
    }

    /// A conflict set by id.
    pub fn conflict(&self, id: &str) -> Option<&ConflictSet> {
        self.conflicts.get(id)
    }

    /// Register a layer-A artifact (the artifact layer's write path — the
    /// artifact body is content-addressed; the store keeps the index record).
    pub fn put_artifact(&mut self, artifact: ArtifactVersion) {
        self.applied_seq = self.applied_seq.max(artifact.created_at);
        self.artifacts
            .insert(artifact.artifact_id.clone(), artifact);
    }

    /// The layer-A artifacts.
    pub fn artifacts(&self) -> &BTreeMap<String, ArtifactVersion> {
        &self.artifacts
    }

    /// `record_read(version_id, seq)` — the `memory_usage` fold input; called
    /// by `retrieve` on every delivered item (R4: reads complete before the
    /// read stamps the version's `last_read_at`).
    pub fn record_read(&mut self, version_id: &str, seq: u64) {
        self.reads
            .entry(version_id.to_string())
            .or_default()
            .push(seq);
    }

    /// The read seqs of `version_id` (`memory_usage.last_read_at` =
    /// `max(reads)`).
    pub fn reads_of(&self, version_id: &str) -> &[u64] {
        self.reads
            .get(version_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    // ── put / write ──────────────────────────────────────────────────────

    /// `put(draft, ctx)` — the write op (§5c.3). Order: provenance → scope →
    /// contract → label → lease → supersession → conflict → store.
    pub fn put(
        &mut self,
        draft: MemoryDraft,
        ctx: &WriteContext,
    ) -> Result<PutOutcome, MemoryError> {
        self.put_inner(draft, ctx, false)
    }

    /// `put_inner(draft, ctx, endorsed)` — `endorsed` waives the supersession
    /// authority check (`authority(new) ≥ authority(old)`) — the promotion
    /// path's `security.label.endorsed` basis substitutes for it (§5c.4;
    /// reachable only from [`crate::lifecycle::promote`], whose own checks
    /// authorize the basis).
    pub(crate) fn put_inner(
        &mut self,
        draft: MemoryDraft,
        ctx: &WriteContext,
        endorsed: bool,
    ) -> Result<PutOutcome, MemoryError> {
        // (1) Provenance is mandatory — MissingProvenance refuses.
        let provenance = draft
            .provenance
            .clone()
            .ok_or(MemoryError::MissingProvenance)?;
        // (2) Scope ceiling — `definition` is `seal`-only at C0; `user` needs
        // approval the delegate-class writer cannot supply.
        // The write ceiling binds agent-authored writes: `model`/`evolution`/
        // `participant` origins are delegate-class (hh-provenance's
        // `is_delegate_class`), and `tool` results write at `delegate` too.
        let delegate_class_writer = provenance.origin.is_delegate_class()
            || matches!(provenance.origin, Origin::Tool { .. });
        match draft.scope {
            PersistenceScope::Definition => {
                return Err(MemoryError::ScopeCeilingExceeded {
                    scope: draft.scope,
                    writer: provenance.authority.as_str().to_string(),
                })
            }
            PersistenceScope::User if delegate_class_writer => {
                return Err(MemoryError::ScopeCeilingExceeded {
                    scope: draft.scope,
                    writer: provenance.authority.as_str().to_string(),
                })
            }
            _ => {}
        }
        let kp = self.policy.per_kind_defaults.get(&draft.kind);
        if let Some(kp) = kp {
            if !kp.allowed_scopes.contains(&draft.scope) {
                return Err(MemoryError::ScopeCeilingExceeded {
                    scope: draft.scope,
                    writer: format!("kind:{}", draft.kind.as_str()),
                });
            }
        }
        // (3) Contract — merge the per-kind default, then C-CONTRACT-1.
        let contract = draft
            .contract
            .clone()
            .or_else(|| kp.and_then(|k| k.default_contract.clone()))
            .unwrap_or(InvalidationContract {
                dependencies: Vec::new(),
                cache_hint: CacheHint::Cacheable,
                validator_ref: None,
                freshness: None,
                invalidation_condition: None,
                revalidation: Revalidation::Never,
            });
        match contract.cache_hint {
            CacheHint::NoStore => {
                return Err(MemoryError::NotPersistable {
                    detail: "cache_hint no_store never persists".to_string(),
                })
            }
            CacheHint::Recompute
                if matches!(
                    draft.scope,
                    PersistenceScope::Session
                        | PersistenceScope::Project
                        | PersistenceScope::User
                        | PersistenceScope::Definition
                ) =>
            {
                return Err(MemoryError::NotPersistable {
                    detail: "cache_hint recompute never persists beyond turn".to_string(),
                })
            }
            _ => {}
        }
        if contract.is_empty()
            && matches!(
                draft.scope,
                PersistenceScope::Session | PersistenceScope::Project | PersistenceScope::User
            )
        {
            return Err(MemoryError::MissingContract { scope: draft.scope });
        }
        // A `table` stamp is legal only when the kind's policy allows.
        if contract
            .dependencies
            .iter()
            .any(|d| d.granularity == Granularity::Table)
            && kp.is_some_and(|k| !k.granularity_allowed)
        {
            return Err(MemoryError::ScopeCeilingExceeded {
                scope: draft.scope,
                writer: format!("kind:{} granularity:table", draft.kind.as_str()),
            });
        }
        // (4) Label — join(context_label, writer_label), then the store caps.
        let mut label = ctx.context_label.join(&provenance.label());
        if delegate_class_writer && label.authority > self.policy.write_ceiling {
            label.authority = self.policy.write_ceiling;
        }
        let text_cap = if draft.content.is_text() && !draft.validator_endorsed {
            hh_provenance::AuthorityClass::External
        } else if draft.validator_endorsed {
            hh_provenance::AuthorityClass::Environment
        } else {
            hh_provenance::AuthorityClass::Kernel
        };
        if delegate_class_writer && label.authority > text_cap {
            label.authority = text_cap;
        }
        if let Some(kp) = kp {
            if label.authority < kp.min_authority_for_storage {
                return Err(MemoryError::AuthorityInsufficient {
                    new_authority: label.authority.as_str().to_string(),
                    old_authority: kp.min_authority_for_storage.as_str().to_string(),
                });
            }
        }
        // (5) Lease — a fenced write refuses.
        let expected = self.lease(draft.scope);
        if ctx.lease_generation != expected {
            return Err(MemoryError::Fenced {
                scope: draft.scope,
                expected,
                got: ctx.lease_generation,
            });
        }
        // (6) Supersession claim — the edge is validated before the version
        // mints (kind match, existence, authority ≥ superseded's).
        if let Some(claim) = &draft.supersedes {
            let old = self.versions.get(&claim.version_id).ok_or_else(|| {
                MemoryError::UnknownVersion {
                    version_id: claim.version_id.clone(),
                }
            })?;
            if old.kind != draft.kind {
                return Err(MemoryError::KindMismatch {
                    newer: draft.kind,
                    older: old.kind,
                });
            }
            if !endorsed && label.authority < old.label.authority {
                return Err(MemoryError::AuthorityInsufficient {
                    new_authority: label.authority.as_str().to_string(),
                    old_authority: old.label.authority.as_str().to_string(),
                });
            }
            if self.lineage.is_revoked(&claim.version_id) {
                // Superseding a revoked version is legal (the new version
                // carries the claim) but the *edge* must not silently
                // resurrect — the old stays revoked.
            }
        }
        let semantic_id = match &draft.semantic_id {
            Some(s) => s.clone(),
            None => idp_id(
                MEMORY_LINE_IDP,
                format!("{}:{}", ctx.run_id, ctx.at_seq).as_bytes(),
            ),
        };
        let mut version = MemoryVersion {
            version_id: String::new(),
            semantic_id,
            kind: draft.kind,
            subject_key: draft.subject_key.clone(),
            content: draft.content.clone(),
            contract: contract.clone(),
            scope: draft.scope,
            label: label.clone(),
            provenance: provenance.clone(),
            validity: draft.validity.clone(),
            declared_inputs: draft.declared_inputs.clone(),
            justifications: draft.justifications.clone(),
            created_at: ctx.at_seq,
            created_by: render_origin(&provenance.origin),
            supersedes_claim: draft.supersedes.clone(),
            conflict_set_ref: None,
            validator_endorsed: draft.validator_endorsed,
        };
        version.version_id = identify_bytes(
            RecordKind::Memory,
            version.body_json().to_canonical_string().as_bytes(),
        );
        // (7) Apply the supersession edge (the claim was validated above).
        if let Some(claim) = &draft.supersedes {
            let ancestor_or_sibling = self.same_line(&version.semantic_id, &claim.version_id);
            self.lineage
                .supersede(
                    &version.version_id,
                    &claim.version_id,
                    claim.reason.lineage_reason(),
                    ancestor_or_sibling || claim.reason == SupersedeClaimReason::Migration,
                )
                .map_err(|e| match e {
                    hh_identity::supersede::SupersedeError::CycleDetected { newer, older } => {
                        MemoryError::CycleDetected { newer, older }
                    }
                    hh_identity::supersede::SupersedeError::NotAncestorOrSibling => {
                        MemoryError::CycleDetected {
                            newer: version.version_id.clone(),
                            older: claim.version_id.clone(),
                        }
                    }
                })?;
        }
        // J1: non-declared-input justifications enter the dependency graph.
        for j in &version.justifications {
            if j.kind != JustificationKind::DeclaredInput {
                self.lineage
                    .declare_dependency(&version.version_id, &j.ref_.version_id);
            }
        }
        // (8) Deterministic conflict detection — same `subject_key`, same
        // schema, different closed-schema `Structured` value, no edge.
        let mut conflict = None;
        if let Some(sk) = &version.subject_key {
            if let MemoryContent::Structured(new_val) = &version.content {
                let mut members = Vec::new();
                for vid in &self.order {
                    let u = &self.versions[vid];
                    if u.subject_key.as_ref() == Some(sk)
                        && !self.lineage.is_revoked(vid)
                        && !self.is_superseded(vid)
                        && version
                            .supersedes_claim
                            .as_ref()
                            .is_none_or(|c| &c.version_id != vid)
                    {
                        if let MemoryContent::Structured(old_val) = &u.content {
                            if old_val.to_canonical_string() != new_val.to_canonical_string() {
                                members.push(vid.clone());
                            }
                        }
                    }
                }
                if !members.is_empty() {
                    members.push(version.version_id.clone());
                    members.sort();
                    members.dedup();
                    let set_id = idp_id(
                        CONFLICT_IDP,
                        format!("{}:{}:{}", sk.schema_ref, sk.key, members.join(",")).as_bytes(),
                    );
                    let set = ConflictSet {
                        conflict_set_id: set_id.clone(),
                        subject_key: sk.clone(),
                        members: members.clone(),
                        detector: "deterministic".to_string(),
                        resolution: if self.policy.escalate_on_unresolved {
                            ConflictResolution::Escalated {
                                principal_ref: self.policy.escalation_principal.clone(),
                            }
                        } else {
                            ConflictResolution::Coexist
                        },
                        escalated_to: if self.policy.escalate_on_unresolved {
                            Some(self.policy.escalation_principal.clone())
                        } else {
                            None
                        },
                    };
                    for m in &members {
                        if let Some(v) = self.versions.get_mut(m) {
                            v.conflict_set_ref = Some(set_id.clone());
                        }
                    }
                    version.conflict_set_ref = Some(set_id.clone());
                    self.conflicts.insert(set_id, set.clone());
                    conflict = Some(set);
                }
            }
        }
        // (9) Store + emit.
        self.applied_seq = self.applied_seq.max(ctx.at_seq);
        self.order.push(version.version_id.clone());
        self.versions
            .insert(version.version_id.clone(), version.clone());
        let event = (
            "context.memory.written".to_string(),
            events::memory_written_payload(&version, ctx),
        );
        self.emitted.push(event.clone());
        Ok(PutOutcome {
            version,
            event,
            conflict,
        })
    }

    fn same_line(&self, semantic_id: &str, version_id: &str) -> bool {
        self.versions
            .get(version_id)
            .map(|v| v.semantic_id == semantic_id)
            .unwrap_or(false)
    }

    fn is_superseded(&self, version_id: &str) -> bool {
        self.lineage.edges().iter().any(|e| e.older == version_id)
    }

    // ── bind / manifest / resolve ────────────────────────────────────────

    /// `bind(scope, name, version_id, supersedes?, reason)` — append to the
    /// name history (§5c.3; never an edit). `supersedes` names the prior
    /// binding's version and applies the lineage edge.
    pub fn bind(
        &mut self,
        scope: PersistenceScope,
        name: &str,
        version_id: &str,
        supersedes: Option<&str>,
        reason: &str,
        at_seq: u64,
    ) -> Result<NameBinding, MemoryError> {
        if !self.versions.contains_key(version_id) {
            return Err(MemoryError::UnknownVersion {
                version_id: version_id.to_string(),
            });
        }
        if let Some(s) = supersedes {
            if !self.versions.contains_key(s) {
                return Err(MemoryError::UnknownVersion {
                    version_id: s.to_string(),
                });
            }
            self.lineage
                .supersede(
                    version_id,
                    s,
                    SupersedeReason::Edit,
                    true, // name-history bindings are siblings on the line
                )
                .map_err(|e| match e {
                    hh_identity::supersede::SupersedeError::CycleDetected { newer, older } => {
                        MemoryError::CycleDetected { newer, older }
                    }
                    _ => MemoryError::CycleDetected {
                        newer: version_id.to_string(),
                        older: s.to_string(),
                    },
                })?;
        }
        let b = NameBinding {
            scope,
            name: name.to_string(),
            version_id: version_id.to_string(),
            bound_at: at_seq,
            supersedes: supersedes.map(str::to_string),
            reason: reason.to_string(),
        };
        self.applied_seq = self.applied_seq.max(at_seq);
        self.names
            .entry(format!("{}:{}", scope.as_str(), name))
            .or_default()
            .push(b.clone());
        Ok(b)
    }

    /// `name_lookup(scope, name)` — the current bound version (the last
    /// binding; `resolve`'s name-selector read).
    pub fn name_lookup(&self, scope: PersistenceScope, name: &str) -> Option<&str> {
        self.names
            .get(&format!("{}:{}", scope.as_str(), name))
            .and_then(|bs| bs.last())
            .map(|b| b.version_id.as_str())
    }

    /// `manifest(scope, at)` — the folded name history (§5c.3): last binding
    /// per name with `bound_at ≤ at`. Content-addressed under
    /// `RecordKind::MemoryManifest`.
    pub fn manifest(&self, scope: PersistenceScope, at: u64) -> MemoryManifest {
        let mut entries = BTreeMap::new();
        let mut keys: Vec<&String> = self
            .names
            .keys()
            .filter(|k| k.starts_with(&format!("{}:", scope.as_str())))
            .collect();
        keys.sort();
        for k in keys {
            let name = &k[scope.as_str().len() + 1..];
            if let Some(last) = self.names[k].iter().rfind(|b| b.bound_at <= at) {
                entries.insert(name.to_string(), last.version_id.clone());
            }
        }
        let body = Json::obj([
            ("scope", Json::str(scope.as_str())),
            ("at_seq", Json::Int(at as i64)),
            (
                "entries",
                Json::Arr(
                    entries
                        .iter()
                        .map(|(n, v)| {
                            Json::obj([
                                ("name", Json::str(n.clone())),
                                ("version_id", Json::str(v.clone())),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]);
        MemoryManifest {
            manifest_id: identify_bytes(
                RecordKind::MemoryManifest,
                body.to_canonical_string().as_bytes(),
            ),
            scope,
            entries,
            at_seq: at,
        }
    }

    /// `resolve(selector, mode)` (§5c.3): name or version selector;
    /// `Execute` never serves revoked/stale/superseded heads (the head's
    /// lifecycle at `applied_seq`); `Audit` returns the record annotated.
    pub fn resolve(
        &self,
        scope: PersistenceScope,
        name: Option<&str>,
        version_id: Option<&str>,
        mode: hh_identity::names::ResolveMode,
    ) -> Result<ResolveOutcome, MemoryError> {
        let id = match (name, version_id) {
            (Some(n), _) => self
                .names
                .get(&format!("{}:{}", scope.as_str(), n))
                .and_then(|bs| bs.last())
                .map(|b| b.version_id.clone())
                .ok_or_else(|| MemoryError::UnknownName {
                    scope,
                    name: n.to_string(),
                })?,
            (None, Some(v)) => v.to_string(),
            (None, None) => {
                return Err(MemoryError::UnknownName {
                    scope,
                    name: String::new(),
                })
            }
        };
        let v = self
            .versions
            .get(&id)
            .ok_or_else(|| MemoryError::UnknownVersion {
                version_id: id.clone(),
            })?;
        let state = crate::lifecycle::lifecycle_state(self, &id, self.applied_seq);
        use hh_identity::names::ResolveMode;
        match mode {
            ResolveMode::Execute => {
                // Execute never serves revoked/stale/superseded heads
                // (§5c.3) — `expired` joins the never-serve set; `valid`,
                // `stale_by_dependency` and `unknown` remain servable (the
                // slot's `admitted_states` decides delivery).
                match state.kind() {
                    LifecycleStateKind::Valid
                    | LifecycleStateKind::StaleByDependency
                    | LifecycleStateKind::Unknown => {
                        Ok(ResolveOutcome::Live { version: v.clone() })
                    }
                    other => Ok(ResolveOutcome::Unservable {
                        version_id: id,
                        state: other,
                    }),
                }
            }
            _ => Ok(ResolveOutcome::Annotated {
                version: v.clone(),
                state: state.kind(),
            }),
        }
    }

    /// `stale_candidates(scope)` — the derived view consolidation reads:
    /// versions in `scope` whose lifecycle at `applied_seq` is
    /// `stale_by_dependency | superseded | expired` (§5c.3).
    pub fn stale_candidates(&self, scope: PersistenceScope) -> Vec<String> {
        self.order
            .iter()
            .filter(|vid| self.versions[*vid].scope == scope)
            .filter(|vid| {
                matches!(
                    crate::lifecycle::lifecycle_state(self, vid, self.applied_seq).kind(),
                    LifecycleStateKind::StaleByDependency
                        | LifecycleStateKind::Superseded
                        | LifecycleStateKind::Expired
                )
            })
            .cloned()
            .collect()
    }

    // ── the contract environment ports (check_contract's `env`) ──────────

    /// `set_stamp(dep_ref, stamp)` — the C0 dependency-stamp source (kernel/
    /// registry records update the current stamp through this port).
    pub fn set_stamp(&mut self, dep_ref: &str, stamp: &str) {
        self.stamps.insert(dep_ref.to_string(), stamp.to_string());
    }

    /// `current_stamp(dep)` — the stamp check's read.
    pub fn current_stamp(&self, dep: &DependencyStamp) -> Option<String> {
        self.stamps.get(&dep.ref_).cloned()
    }

    /// `set_validator_verdict(version_id, validator_ref, ok)` — the
    /// validator port (the last verdict `check_contract` reads).
    pub fn set_validator_verdict(&mut self, version_id: &str, validator_ref: &str, ok: bool) {
        self.verdicts
            .insert((version_id.to_string(), validator_ref.to_string()), ok);
    }

    /// `validator_verdict(version_id, validator_ref)`.
    pub fn validator_verdict(&self, version_id: &str, validator_ref: &str) -> Option<bool> {
        self.verdicts
            .get(&(version_id.to_string(), validator_ref.to_string()))
            .copied()
    }

    /// `mark_scope_ended(scope)` — `invalidation_condition: scope_ended` fires.
    pub fn mark_scope_ended(&mut self, scope: PersistenceScope) {
        self.ended_scopes.insert(scope);
    }

    /// `publish_replacement(name)` — `invalidation_condition:
    /// replacement_published{name}` fires.
    pub fn publish_replacement(&mut self, name: &str) {
        self.published_replacements.insert(name.to_string());
    }

    /// `condition_fired(ic, v)` — the caller-evaluated conditions
    /// (`scope_ended`, `replacement_published`, `superseded`).
    pub fn condition_fired(&self, ic: &InvalidationCondition, v: &MemoryVersion) -> bool {
        match ic {
            InvalidationCondition::ScopeEnded(s) => self.ended_scopes.contains(s),
            InvalidationCondition::ReplacementPublished(n) => {
                self.published_replacements.contains(n)
            }
            InvalidationCondition::Superseded => self.is_superseded(&v.version_id),
            _ => false,
        }
    }

    /// The store-side accessors the lifecycle/retrieval folds read.
    pub(crate) fn lineage_ref(&self) -> &Lineage {
        &self.lineage
    }

    /// Mutable lineage access for `revoke` (kept crate-private — writes go
    /// through the checked ops).
    pub(crate) fn lineage_mut(&mut self) -> &mut Lineage {
        &mut self.lineage
    }

    /// Record a `MemoryRevocation` row (called by `lifecycle::revoke` after
    /// the lineage update).
    pub(crate) fn push_revocation(&mut self, r: MemoryRevocation) {
        self.applied_seq = self.applied_seq.max(r.at_seq);
        self.revocations.push(r);
    }

    /// Push an emitted payload row (used by `lifecycle` ops).
    pub(crate) fn emit(&mut self, class: &str, payload: Json) {
        self.emitted.push((class.to_string(), payload));
    }

    /// Touch `applied_seq` (used by `lifecycle` ops).
    pub(crate) fn touch(&mut self, seq: u64) {
        self.applied_seq = self.applied_seq.max(seq);
    }

    /// Insert a resolved `ConflictSet`.
    pub(crate) fn put_conflict(&mut self, set: ConflictSet) {
        self.conflicts.insert(set.conflict_set_id.clone(), set);
    }

    /// All versions (the fold input — read-only).
    pub fn versions(&self) -> &BTreeMap<String, MemoryVersion> {
        &self.versions
    }

    /// `enumerate(layer)` — the raw (unfiltered) hit ids a query admits in
    /// `layer` (§5c.3 `enumerate(layer, query, at)` — the pipeline step).
    /// Filter/rank/budget live in [`crate::retrieve::retrieve`].
    pub(crate) fn raw_ids(&self, layer: Layer) -> Vec<String> {
        match layer {
            Layer::Artifact => self.artifacts.keys().cloned().collect(),
            // E/P/S all fold over memory versions at C0 — the layer member is
            // a *kind* predicate: P ↔ procedure_pointer, S ↔ bound names +
            // session-scope versions, E ↔ the rest.
            Layer::Procedural => self
                .order
                .iter()
                .filter(|v| self.versions[*v].kind == MemoryKind::ProcedurePointer)
                .cloned()
                .collect(),
            Layer::Session => self
                .order
                .iter()
                .filter(|v| {
                    let m = &self.versions[*v];
                    m.scope == PersistenceScope::Session
                        || matches!(m.kind, MemoryKind::Decision | MemoryKind::ToolResult)
                })
                .cloned()
                .collect(),
            Layer::Episodic => self
                .order
                .iter()
                .filter(|v| {
                    let m = &self.versions[*v];
                    m.kind != MemoryKind::ProcedurePointer
                        && m.scope != PersistenceScope::Session
                        && !matches!(m.kind, MemoryKind::Decision | MemoryKind::ToolResult)
                })
                .cloned()
                .collect(),
        }
    }
}

/// Render an `Origin` for record bodies (`created_by`, revocation `record`) —
/// `tag` plus the origin's primary identity coordinate (never the whole
/// provenance record — `origin(origin_k)` refs are refused at the store).
pub fn render_origin(o: &Origin) -> String {
    let detail = match o {
        Origin::Human { author_ref, .. } => author_ref.clone(),
        Origin::Model { model_ref, .. } => model_ref.clone(),
        Origin::Tool { capability, .. } => capability.clone(),
        Origin::Evolution { candidate_id, .. } => candidate_id.clone(),
        Origin::Import { source_system, .. } => source_system.clone(),
        Origin::Migration { from_dialect } => from_dialect.clone(),
        Origin::Kernel { component_ref } => component_ref.clone(),
        Origin::Participant {
            participant_ref, ..
        } => participant_ref.clone(),
    };
    format!("{}({})", o.tag(), detail)
}

/// The `VersionedRef` rendering used inside record bodies (`kind-tag:id`).
pub fn vref_str(vr: &VersionedRef) -> String {
    format!("{}:{}", vr.kind.domain_tag(), vr.version_id)
}

fn vref_json_str(vr: &VersionedRef) -> Json {
    Json::str(vref_str(vr))
}
