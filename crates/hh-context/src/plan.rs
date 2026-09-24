//! §5c.1 data model: `AssemblyRequest`'s candidate/budget records, the
//! `Layout`/`SlotDeclaration` shape with the six kernel-reserved slots and
//! floors, `link_layout` (LC-1/LC-3 refused at link — AC-R-2.4.1-14), the
//! `ContextPlan` record with its content-addressed `plan_id` (I-DET), and the
//! `OffloadHandle` machinery (excerpt + read-capability, `HandleUnreadable`).
//!
//! Records not redefined here (CC7): `Label`/`ProvenanceRecord`/
//! `AuthorityClass`/`PersistenceScope` (`hh-provenance`), `EventRef`
//! (`hh-ledger::manifest`), `Validity` (`hh-hir::records`).

use std::collections::BTreeSet;

use hh_hir::records::Validity;
use hh_identity::idp::idp_id;
use hh_ledger::manifest::EventRef;
use hh_provenance::label::Label;
use hh_provenance::record::ProvenanceRecord;
use hh_provenance::AuthorityClass;
use hh_wire::json::Json;

use crate::vocab::{
    CacheTier, CandidateKind, CandidateState, ConflictPolicy, LifecycleStateKind, OmissionReason,
    Retention, CANDIDATE_KINDS, VOLATILE_KINDS,
};

/// `context_plan/1` — the `idp` domain for plan identity. The preimage is the
/// canonical plan minus `plan_id`; two identical inputs produce identical ids
/// (I-DET; `estimate` and `estimator_ref` are recorded inputs of the hash —
/// AC-R-2.4.1-8).
pub const PLAN_IDP: &str = "context_plan.1";

/// `context_delivery/1` — the `idp` domain for per-call delivery ids (fresh
/// per call, deterministic under identical inputs).
pub const DELIVERY_IDP: &str = "context_delivery.1";

/// `context_omission_item/1` — the `idp` domain for the kernel-authored
/// omission item's `ContextItem` identity (content-addressed so identical
/// omission sets produce identical plans).
pub const OMISSION_ITEM_IDP: &str = "context_omission_item.1";

/// `context_static.1` — the `idp` domain for the per-call `static_hash` (the
/// LC-2 run half — identical static-slot content across consecutive calls is
/// the caller's check).
pub const STATIC_HASH_IDP: &str = "context_static.1";

/// `context_offload/1` — the `idp` domain for offload content addresses.
pub const OFFLOAD_IDP: &str = "context_offload.1";

// ─────────────────────────────────────────────────────────────────────────────
// Candidates, budget, estimates
// ─────────────────────────────────────────────────────────────────────────────

/// `estimate{tokens, estimator_ref}` — the pinned estimator's output; an input
/// of the plan hash (I-DET), never kernel-truncation data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Estimate {
    /// The estimated token cost (all-in: content + framing).
    pub tokens: u64,
    /// `Ref` to the pinned estimator in the call profile.
    pub estimator_ref: String,
}

/// `Candidate` — one slot-competing item the `AssemblyRequest` carries
/// (§5c.1 data model). `label`/`provenance` are mandatory at admission —
/// `MissingProvenance` never defaults, `Unidentified` when no compiled
/// identity or content address exists (I-ID).
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Fresh kernel-generated id.
    pub candidate_id: String,
    /// `context_item_id | artefact_id` — the compiled identity / content
    /// address; `None` fires `unidentified` (I-ID; `UndeliverableArtifact`
    /// under `required` retention).
    pub context_item_id: Option<String>,
    /// The closed kind.
    pub kind: CandidateKind,
    /// `handle_only | expanded`.
    pub state: CandidateState,
    /// `required | optional(PriorityClass)` — set by the kernel or a
    /// definition-level `HarnessRule`; a policy may never edit it (I-NOWIDEN).
    pub retention: Retention,
    /// The pinned estimator's output.
    pub estimate: Estimate,
    /// The ledger event the item is derived from (`source_event?` — None only
    /// for kernel-fabricated items like the omission item).
    pub source_event: Option<EventRef>,
    /// The ledger order of `source_event` in the folded view — the `age`
    /// input the deterministic eviction order reads.
    pub source_seq: u64,
    /// `label: Label` — mandatory.
    pub label: Label,
    /// `provenance: ProvenanceRecord` — mandatory.
    pub provenance: ProvenanceRecord,
    /// The item's validity window (the `Validity` of `hh-hir` — evaluated at
    /// the request's `at_seq` for I-ORDER).
    pub validity: Validity,
    /// `readers?` — carried at C0 (enforced at the slot boundary C2). When
    /// present it narrows the effective reader set; when absent the label's
    /// readers carry.
    pub readers: Option<hh_provenance::authority::ReaderSet>,
    /// `slot_hint?` — advisory only (§5c.1); the slot decision is
    /// authority/lifecycle-driven. Never binding.
    pub slot_hint: Option<String>,
    /// `volatile` — the candidate-level volatile marker (a `kernel_notice`
    /// carrying `current_time`, an occupancy item). A volatile candidate
    /// placed in a `static` slot is `VolatileInStaticTier` (LC-3's run half).
    pub volatile: bool,
    /// `paired_with` — I-PAIR: the candidate_id this item is indivisible with
    /// (a call and its observation).
    pub paired_with: Option<String>,
    /// `batch_id` — I-ATOM: a declared batch is atomic (a `tool_result` batch
    /// is retained or evicted whole).
    pub batch_id: Option<String>,
    /// `artefact_id` — when the item is a harness artifact (drives
    /// `context.artefact.delivered` emission).
    pub artefact_id: Option<String>,
    /// `handle` — the offload handle for `handle_only` deliveries (carries
    /// `read_capability`; `HandleUnreadable` is a typed failure, never a
    /// silent skip).
    pub handle: Option<OffloadHandle>,
}

impl Candidate {
    /// The canonical JSON (strict-codec paired — the candidate shape is closed).
    pub fn to_json(&self) -> Json {
        let mut v = vec![
            ("candidate_id", Json::str(self.candidate_id.clone())),
            ("kind", Json::str(self.kind.as_str())),
            ("state", Json::str(self.state.as_str())),
            (
                "retention",
                match self.retention {
                    Retention::Required => Json::str("required"),
                    Retention::Optional(p) => Json::obj([("optional", Json::str(p.as_str()))]),
                },
            ),
            ("estimate", estimate_json(&self.estimate)),
            ("source_seq", Json::Int(self.source_seq as i64)),
            ("label", crate::codec::label_json(&self.label)),
            ("provenance", self.provenance.to_json()),
            ("validity", crate::codec::validity_json(&self.validity)),
            ("volatile", Json::Bool(self.volatile)),
        ];
        if let Some(id) = &self.context_item_id {
            v.push(("context_item_id", Json::str(id.clone())));
        }
        if let Some(ev) = &self.source_event {
            v.push(("source_event", Json::str(crate::codec::event_ref_str(ev))));
        }
        if let Some(r) = &self.readers {
            v.push(("readers", crate::codec::readers_json(r)));
        }
        if let Some(h) = &self.slot_hint {
            v.push(("slot_hint", Json::str(h.clone())));
        }
        if let Some(p) = &self.paired_with {
            v.push(("paired_with", Json::str(p.clone())));
        }
        if let Some(b) = &self.batch_id {
            v.push(("batch_id", Json::str(b.clone())));
        }
        if let Some(a) = &self.artefact_id {
            v.push(("artefact_id", Json::str(a.clone())));
        }
        if let Some(h) = &self.handle {
            v.push(("handle", h.to_json()));
        }
        Json::obj(v)
    }
}

fn estimate_json(e: &Estimate) -> Json {
    Json::obj([
        ("tokens", Json::Int(e.tokens as i64)),
        ("estimator_ref", Json::str(e.estimator_ref.clone())),
    ])
}

/// `Reservation{holder, tokens}` — a declared budget reservation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reservation {
    /// The reserved-for.
    pub holder: String,
    /// The reserved token count.
    pub tokens: u64,
}

/// `ContextBudget{window_cap, margin, reservations[], hard}` — the call's
/// token envelope (§5c.1). `hard` is `true` when the account declares it a
/// hard cap — `matched_budget_or_refuse` reads it (CC9).
#[derive(Debug, Clone)]
pub struct ContextBudget {
    /// `window_cap` — the hard ceiling in tokens.
    pub window_cap: u64,
    /// `margin` — the reserved margin (defensive boundary: `window_cap` is
    /// met *with the margin* — the effective ceiling is `cap - margin`).
    pub margin: u64,
    /// Declared reservations (`defended_head`, `pinned_indices`, `output_budget`,
    /// `watermark`, the omission-item reserve the kernel adds).
    pub reservations: Vec<Reservation>,
    /// `hard` — when the account declares a hard cap.
    pub hard: bool,
}

impl ContextBudget {
    /// The effective ceiling (`window_cap - margin`, saturating).
    pub fn effective_cap(&self) -> u64 {
        self.window_cap.saturating_sub(self.margin)
    }

    /// The reserved total (the declaration `holder`s carry).
    pub fn reserved_total(&self) -> u64 {
        self.reservations.iter().map(|r| r.tokens).sum()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Layout / SlotDeclaration / link checks
// ─────────────────────────────────────────────────────────────────────────────

/// `validity_policy{admitted_states ⊆ {valid, stale_by_dependency, unknown},
///  conflict_policy, max_stale?}` (§5c.1 slot declaration; §5c.3 filter copy).
#[derive(Debug, Clone)]
pub struct ValidityPolicy {
    /// The lifecycle states the slot admits — `superseded`/`revoked`/`expired`
    /// may not appear here (`InvalidLayout` at link).
    pub admitted_states: BTreeSet<LifecycleStateKind>,
    /// How unresolved conflict sets are delivered into this slot.
    pub conflict_policy: ConflictPolicy,
    /// `max_stale` — the bound on `stale_by_dependency` admission (`None` =
    /// unbounded, within `admitted_states`).
    pub max_stale: Option<u64>,
}

/// `SlotOrder{precedence, within_slot}` — the slot's render precedence and the
/// `ComparatorRef` ordering its items.
#[derive(Debug, Clone)]
pub struct SlotOrder {
    /// The render precedence (lower renders earlier; reserved-slot precedence
    /// is the kernel order `kernel < definition < principal < transcript <
    /// external < unverified`).
    pub precedence: u64,
    /// `Ref` to a comparator in the sealed definition (`within_slot` —
    /// `ComparatorRef`, never an inline closure).
    pub within_slot: String,
}

/// `Cardinality{max}` — `None` = unbounded.
#[derive(Debug, Clone)]
pub struct Cardinality {
    /// The maximum item count.
    pub max: Option<u64>,
}

/// `SlotDeclaration{slot_id, min_authority, validity_policy, admits,
/// cardinality, cache_tier, diffable, budget_share, order}` (§5c.1).
#[derive(Debug, Clone)]
pub struct SlotDeclaration {
    /// The slot id.
    pub slot_id: String,
    /// `min_authority` — the floor: an item is admissible only when
    /// `label.authority >= min_authority`. For the six reserved slots the
    /// floor is kernel-fixed (a profile may not lower it — `InvalidLayout`).
    pub min_authority: AuthorityClass,
    /// The slot's validity policy (E2).
    pub validity_policy: ValidityPolicy,
    /// `admits` — the closed `CandidateKind` set the slot accepts.
    pub admits: BTreeSet<CandidateKind>,
    /// `cardinality`.
    pub cardinality: Cardinality,
    /// `cache_tier ∈ {static, dynamic, transcript}` (LC-1).
    pub cache_tier: CacheTier,
    /// `diffable` — whether a compaction diff may rewrite this slot's prefix.
    pub diffable: bool,
    /// `budget_share?` — a declared per-slot token cap (advisory profile data;
    /// enforced as a slot-local cap at `enforce_budget` when present).
    pub budget_share: Option<u64>,
    /// `order`.
    pub order: SlotOrder,
}

/// A `(slot_id, floor, cache_tier)` reserved-slot declaration (§5c.1;
/// ADR-0074): every layout carries all six; the floor may not be lowered;
/// `kernel`/`definition` are `static`, `transcript` is `transcript`, the rest
/// default `dynamic`.
pub const RESERVED_SLOTS: &[(&str, AuthorityClass, CacheTier)] = &[
    ("kernel", AuthorityClass::Kernel, CacheTier::Static),
    ("definition", AuthorityClass::Definition, CacheTier::Static),
    ("principal", AuthorityClass::Principal, CacheTier::Dynamic),
    (
        "transcript",
        AuthorityClass::Delegate,
        CacheTier::Transcript,
    ),
    ("external", AuthorityClass::External, CacheTier::Dynamic),
    ("unverified", AuthorityClass::Unverified, CacheTier::Dynamic),
];

/// `Layout{slots[]}` — the profile-owned slot declarations plus the layout's
/// volatile-kind extension (`volatile_kinds ⊇` the kernel-fixed
/// [`VOLATILE_KINDS`] — LC-3 reads the union).
#[derive(Debug, Clone)]
pub struct Layout {
    /// The slot declarations (must carry all six reserved ids).
    pub slots: Vec<SlotDeclaration>,
    /// The profile's volatile-kind extension — must be a superset of
    /// `VOLATILE_KINDS`; a `static` slot admitting any member is
    /// `VolatileInStaticTier` at link.
    pub volatile_kinds: BTreeSet<CandidateKind>,
}

impl Layout {
    /// The slot declaration, by id.
    pub fn slot(&self, slot_id: &str) -> Option<&SlotDeclaration> {
        self.slots.iter().find(|s| s.slot_id == slot_id)
    }

    /// The slot declaration, mutable (profile-edit paths — link re-checks).
    pub fn slot_mut(&mut self, slot_id: &str) -> Option<&mut SlotDeclaration> {
        self.slots.iter_mut().find(|s| s.slot_id == slot_id)
    }

    /// Whether `kind` is volatile under this layout (kernel floor ∪ extension).
    pub fn is_volatile_kind(&self, kind: CandidateKind) -> bool {
        VOLATILE_KINDS.contains(&kind) || self.volatile_kinds.contains(&kind)
    }
}

/// The `link_layout` failures (§5c.1; AC-R-2.4.1-14: LC-1/LC-3 refused at link).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    /// `InvalidLayout` — a layout missing a reserved slot, a floor below the
    /// reserved floor (or below `unverified` for any slot), a tier break
    /// (a `static` slot placed after a lower-ranked tier), an
    /// `admitted_states` subset containing `superseded|revoked|expired`, a
    /// non-transcript slot admitting `transcript_item`, or an admits set
    /// naming an unregistered kind (unrepresentable in the closed sum).
    InvalidLayout {
        /// What failed.
        detail: String,
    },
    /// `VolatileInStaticTier` — a `static` slot admitting a volatile kind
    /// (LC-3 at link).
    VolatileInStaticTier {
        /// The slot.
        slot_id: String,
        /// The volatile kind.
        kind: CandidateKind,
    },
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutError::InvalidLayout { detail } => write!(f, "InvalidLayout: {detail}"),
            LayoutError::VolatileInStaticTier { slot_id, kind } => write!(
                f,
                "VolatileInStaticTier: {slot_id} admits volatile kind {}",
                kind.as_str()
            ),
        }
    }
}

impl std::error::Error for LayoutError {}

/// `link_layout` — the compile-time link checks (§5c.1 LC-1/LC-3;
/// AC-R-2.4.1-14):
///
/// - every layout carries all six reserved slots with their kernel floors
///   (`min_authority` never below the floor);
/// - every slot's `min_authority >= unverified`;
/// - `admitted_states` may not contain `superseded`, `revoked` or `expired`;
/// - a non-`transcript` slot may not admit `transcript_item` (LC-1: the
///   `transcript` tier is the only tier `transcript_item` may occupy);
/// - `kernel`/`definition`/`transcript` carry their kernel-fixed tiers;
/// - a `static` slot admits no volatile kind (LC-3).
pub fn link_layout(layout: &Layout) -> Result<(), LayoutError> {
    for (slot_id, floor, tier) in RESERVED_SLOTS {
        let slot = layout
            .slot(slot_id)
            .ok_or_else(|| LayoutError::InvalidLayout {
                detail: format!("missing reserved slot {slot_id}"),
            })?;
        if slot.min_authority < *floor {
            return Err(LayoutError::InvalidLayout {
                detail: format!(
                    "slot {slot_id} floor {} below the reserved floor {}",
                    slot.min_authority.as_str(),
                    floor.as_str()
                ),
            });
        }
        // kernel/definition/transcript cache tiers are kernel-fixed.
        if (slot_id == &"kernel" || slot_id == &"definition" || slot_id == &"transcript")
            && slot.cache_tier != *tier
        {
            return Err(LayoutError::InvalidLayout {
                detail: format!(
                    "slot {slot_id} cache_tier must be {} (kernel-fixed)",
                    tier.as_str()
                ),
            });
        }
    }
    for slot in &layout.slots {
        if slot.min_authority < AuthorityClass::Unverified {
            return Err(LayoutError::InvalidLayout {
                detail: format!(
                    "slot {} floor {} below unverified",
                    slot.slot_id,
                    slot.min_authority.as_str()
                ),
            });
        }
        for st in &slot.validity_policy.admitted_states {
            match st {
                LifecycleStateKind::Valid
                | LifecycleStateKind::StaleByDependency
                | LifecycleStateKind::Unknown => {}
                bad => {
                    return Err(LayoutError::InvalidLayout {
                        detail: format!(
                            "slot {} admits lifecycle state {}",
                            slot.slot_id,
                            bad.as_str()
                        ),
                    })
                }
            }
        }
        if slot.slot_id != "transcript" && slot.admits.contains(&CandidateKind::TranscriptItem) {
            return Err(LayoutError::InvalidLayout {
                detail: format!(
                    "non-transcript slot {} admits transcript_item",
                    slot.slot_id
                ),
            });
        }
        for kind in &slot.admits {
            if slot.cache_tier == CacheTier::Static && layout.is_volatile_kind(*kind) {
                return Err(LayoutError::VolatileInStaticTier {
                    slot_id: slot.slot_id.clone(),
                    kind: *kind,
                });
            }
        }
    }
    Ok(())
}

/// The kernel's default layout (§5c.1): the six reserved slots in kernel
/// precedence order, `kernel`/`definition` `static`, `transcript` `transcript`,
/// the rest `dynamic`; the transcript slot admits `transcript_item`/`observation`/
/// `subagent_result`; `definition` admits the sealed-definition kinds;
/// `external` admits the discovered/open-world kinds plus the volatile
/// advisory kinds; `unverified` is the lifted/imported fallback.
pub fn default_layout() -> Layout {
    let policy = |states: &[LifecycleStateKind], cp: ConflictPolicy| ValidityPolicy {
        admitted_states: states.iter().copied().collect(),
        conflict_policy: cp,
        max_stale: None,
    };
    let admits = |kinds: &[CandidateKind]| kinds.iter().copied().collect();
    Layout {
        volatile_kinds: VOLATILE_KINDS.iter().copied().collect(),
        slots: vec![
            SlotDeclaration {
                slot_id: "kernel".into(),
                min_authority: AuthorityClass::Kernel,
                validity_policy: policy(
                    &[LifecycleStateKind::Valid],
                    ConflictPolicy::DeliverAllAnnotated,
                ),
                admits: admits(&[CandidateKind::KernelNotice]),
                cardinality: Cardinality { max: None },
                cache_tier: CacheTier::Static,
                diffable: false,
                budget_share: None,
                order: SlotOrder {
                    precedence: 0,
                    within_slot: "comparator/kernel".into(),
                },
            },
            SlotDeclaration {
                slot_id: "definition".into(),
                min_authority: AuthorityClass::Definition,
                validity_policy: policy(
                    &[LifecycleStateKind::Valid],
                    ConflictPolicy::DeliverAllAnnotated,
                ),
                admits: admits(&[
                    CandidateKind::DefinitionInstruction,
                    CandidateKind::ProcedureIndex,
                    CandidateKind::ProcedureBody,
                ]),
                cardinality: Cardinality { max: None },
                cache_tier: CacheTier::Static,
                diffable: false,
                budget_share: None,
                order: SlotOrder {
                    precedence: 1,
                    within_slot: "comparator/definition".into(),
                },
            },
            SlotDeclaration {
                slot_id: "principal".into(),
                min_authority: AuthorityClass::Principal,
                validity_policy: policy(
                    &[LifecycleStateKind::Valid],
                    ConflictPolicy::DeliverAllAnnotated,
                ),
                admits: admits(&[CandidateKind::PrincipalMessage]),
                cardinality: Cardinality { max: None },
                cache_tier: CacheTier::Dynamic,
                diffable: false,
                budget_share: None,
                order: SlotOrder {
                    precedence: 2,
                    within_slot: "comparator/principal".into(),
                },
            },
            SlotDeclaration {
                slot_id: "transcript".into(),
                min_authority: AuthorityClass::Delegate,
                validity_policy: policy(
                    &[LifecycleStateKind::Valid],
                    ConflictPolicy::DeliverAllAnnotated,
                ),
                admits: admits(&[
                    CandidateKind::TranscriptItem,
                    CandidateKind::Observation,
                    CandidateKind::SubagentResult,
                ]),
                cardinality: Cardinality { max: None },
                cache_tier: CacheTier::Transcript,
                diffable: true,
                budget_share: None,
                order: SlotOrder {
                    precedence: 3,
                    within_slot: "comparator/ledger_order".into(),
                },
            },
            SlotDeclaration {
                slot_id: "external".into(),
                min_authority: AuthorityClass::External,
                validity_policy: policy(
                    &[
                        LifecycleStateKind::Valid,
                        LifecycleStateKind::StaleByDependency,
                        LifecycleStateKind::Unknown,
                    ],
                    ConflictPolicy::DeliverAllAnnotated,
                ),
                admits: admits(&[
                    CandidateKind::Observation,
                    CandidateKind::ArtifactExcerpt,
                    CandidateKind::Memory,
                    CandidateKind::MemoryIndex,
                    CandidateKind::ToolSurface,
                    CandidateKind::ProcedureIndex,
                    CandidateKind::ProcedureBody,
                    CandidateKind::BudgetReminder,
                    CandidateKind::EnvironmentState,
                    CandidateKind::DefinitionInstruction,
                    CandidateKind::KernelNotice,
                ]),
                cardinality: Cardinality { max: None },
                cache_tier: CacheTier::Dynamic,
                diffable: true,
                budget_share: None,
                order: SlotOrder {
                    precedence: 4,
                    within_slot: "comparator/ledger_order".into(),
                },
            },
            SlotDeclaration {
                slot_id: "unverified".into(),
                min_authority: AuthorityClass::Unverified,
                validity_policy: policy(
                    &[
                        LifecycleStateKind::Valid,
                        LifecycleStateKind::StaleByDependency,
                        LifecycleStateKind::Unknown,
                    ],
                    ConflictPolicy::DeliverAllAnnotated,
                ),
                admits: CANDIDATE_KINDS
                    .iter()
                    .copied()
                    .filter(|k| *k != CandidateKind::TranscriptItem)
                    .collect(),
                cardinality: Cardinality { max: None },
                cache_tier: CacheTier::Dynamic,
                diffable: true,
                budget_share: None,
                order: SlotOrder {
                    precedence: 5,
                    within_slot: "comparator/ledger_order".into(),
                },
            },
        ],
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ContextPlan
// ─────────────────────────────────────────────────────────────────────────────

/// `Omission{candidate_id, reason, evidence}` — everything withheld is
/// accounted (§5c.1 data model; CC3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Omission {
    /// The withheld candidate.
    pub candidate_id: String,
    /// The closed reason.
    pub reason: OmissionReason,
    /// A pointer to the audit evidence (the emit row / the check that fired).
    pub evidence: String,
}

/// `PlannedItem{candidate_id, context_item_id, artefact_id?, delivery_id,
/// authority, label, tokens, state, delivered_by_reference, derived_from?}`
/// (§5c.1 data model).
#[derive(Debug, Clone)]
pub struct PlannedItem {
    /// The admitted candidate.
    pub candidate_id: String,
    /// The delivered `ContextItem` identity (the candidate's compiled id, or
    /// the omission item's minted id).
    pub context_item_id: String,
    /// `artefact_id` — when the delivery is a harness artifact.
    pub artefact_id: Option<String>,
    /// `delivery_id` — fresh per call.
    pub delivery_id: String,
    /// The delivered authority (the item's label authority — never widened).
    pub authority: AuthorityClass,
    /// The delivered label.
    pub label: Label,
    /// The plan-counted tokens (the estimate under the pinned estimator).
    pub tokens: u64,
    /// `handle_only | expanded` as delivered.
    pub state: CandidateState,
    /// `delivered_by_reference` — a handle the model expands.
    pub delivered_by_reference: bool,
    /// `derived_from` — a compaction summary's `derived_from` when the item
    /// replaced a compacted range.
    pub derived_from: Option<String>,
}

/// `SlotFill{slot_id, items[]}`.
#[derive(Debug, Clone)]
pub struct SlotFill {
    /// The slot.
    pub slot_id: String,
    /// The delivered items in render order.
    pub items: Vec<PlannedItem>,
}

/// `legal_cut_points[]` — flattened-item indices before which a compaction
/// cut is legal (every slot boundary; inside the transcript tier, between two
/// items where neither is `paired_with` the other — I-PAIR).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CutPoint {
    /// The flattened index a cut may precede.
    pub before_index: u64,
}

/// `ContextPlan{plan_id, model_call_id, derived_from, slots[], omitted[],
/// context_label, occupancy_estimate, legal_cut_points[], reserved,
/// estimator_ref}` — the immutable content-addressed output (§5c.1 data model;
/// R-WIRE). `plan_id` is `idp(context_plan.1, canonical(plan \ {plan_id}))`.
#[derive(Debug, Clone)]
pub struct ContextPlan {
    /// The content address.
    pub plan_id: String,
    /// `model_call_id`.
    pub model_call_id: String,
    /// `derived_from` — the projected watermark `{run_id, seq, view_hash}`.
    pub derived_from: DerivedFrom,
    /// The slot fills in render order.
    pub slots: Vec<SlotFill>,
    /// The omission accounting.
    pub omitted: Vec<Omission>,
    /// `context_label` — the join over delivered items' labels (handle-only
    /// items contribute nothing — I-LABEL).
    pub context_label: Label,
    /// `occupancy_estimate` — Σ delivered tokens.
    pub occupancy_estimate: u64,
    /// `legal_cut_points`.
    pub legal_cut_points: Vec<CutPoint>,
    /// `reserved` — the reservation total the plan carries.
    pub reserved: u64,
    /// `estimator_ref` — the pinned estimator identity (I-DET input).
    pub estimator_ref: String,
    /// `static_hash` — the LC-2 run half: `idp` over the static slots'
    /// delivered item content ids; the caller compares consecutive calls.
    pub static_hash: String,
}

/// `derived_from` — the projected watermark the plan answers.
#[derive(Debug, Clone)]
pub struct DerivedFrom {
    /// The run.
    pub run_id: String,
    /// The folded seq.
    pub seq: u64,
    /// `view_hash`.
    pub view_hash: String,
}

impl ContextPlan {
    /// The canonical JSON minus `plan_id` — the `plan_id` preimage.
    pub fn body_json(&self) -> Json {
        let mut slots = Vec::new();
        for f in &self.slots {
            let mut items = Vec::new();
            for it in &f.items {
                let mut v = vec![
                    ("candidate_id", Json::str(it.candidate_id.clone())),
                    ("context_item_id", Json::str(it.context_item_id.clone())),
                    ("delivery_id", Json::str(it.delivery_id.clone())),
                    ("authority", Json::str(it.authority.as_str())),
                    ("label", crate::codec::label_json(&it.label)),
                    ("tokens", Json::Int(it.tokens as i64)),
                    ("state", Json::str(it.state.as_str())),
                    (
                        "delivered_by_reference",
                        Json::Bool(it.delivered_by_reference),
                    ),
                ];
                if let Some(a) = &it.artefact_id {
                    v.push(("artefact_id", Json::str(a.clone())));
                }
                if let Some(d) = &it.derived_from {
                    v.push(("derived_from", Json::str(d.clone())));
                }
                items.push(Json::obj(v));
            }
            slots.push(Json::obj([
                ("slot_id", Json::str(f.slot_id.clone())),
                ("items", Json::Arr(items)),
            ]));
        }
        Json::obj([
            ("model_call_id", Json::str(self.model_call_id.clone())),
            (
                "derived_from",
                Json::obj([
                    ("run_id", Json::str(self.derived_from.run_id.clone())),
                    ("seq", Json::Int(self.derived_from.seq as i64)),
                    ("view_hash", Json::str(self.derived_from.view_hash.clone())),
                ]),
            ),
            ("slots", Json::Arr(slots)),
            (
                "omitted",
                Json::Arr(
                    self.omitted
                        .iter()
                        .map(|o| {
                            Json::obj([
                                ("candidate_id", Json::str(o.candidate_id.clone())),
                                ("reason", Json::str(o.reason.as_str())),
                                ("evidence", Json::str(o.evidence.clone())),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "context_label",
                crate::codec::label_json(&self.context_label),
            ),
            (
                "occupancy_estimate",
                Json::Int(self.occupancy_estimate as i64),
            ),
            (
                "legal_cut_points",
                Json::Arr(
                    self.legal_cut_points
                        .iter()
                        .map(|c| Json::Int(c.before_index as i64))
                        .collect(),
                ),
            ),
            ("reserved", Json::Int(self.reserved as i64)),
            ("estimator_ref", Json::str(self.estimator_ref.clone())),
            ("static_hash", Json::str(self.static_hash.clone())),
        ])
    }

    /// Mint `plan_id` over the body (idempotent — `set_plan_id` computes it).
    pub fn set_plan_id(&mut self) {
        self.plan_id = idp_id(PLAN_IDP, self.body_json().to_canonical_string().as_bytes());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Offload machinery (handle_only ⇔ indexed; excerpt + read_capability)
// ─────────────────────────────────────────────────────────────────────────────

/// `ExcerptPolicy{max_lines?, max_bytes?, max_tokens, mode}` — the slot's
/// excerpt rule (§5c.1 ADR-0075).
#[derive(Debug, Clone)]
pub struct ExcerptPolicy {
    /// Line bound.
    pub max_lines: Option<u64>,
    /// Byte bound.
    pub max_bytes: Option<u64>,
    /// Token bound.
    pub max_tokens: u64,
    /// `head | tail | head_tail`.
    pub mode: ExcerptMode,
}

/// `ExcerptPolicy.mode ∈ {head, tail, head_tail}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExcerptMode {
    /// Keep the head.
    Head,
    /// Keep the tail.
    Tail,
    /// Keep head + tail, elide the middle.
    HeadTail,
}

/// `excerpt_report{truncated_by, total_lines, total_bytes, retained_range}` —
/// carried on every excerpted delivery (§5c.1).
#[derive(Debug, Clone)]
pub struct ExcerptReport {
    /// Which bound cut (`lines | bytes | tokens`).
    pub truncated_by: Option<String>,
    /// The full content's line count.
    pub total_lines: u64,
    /// The full content's byte count.
    pub total_bytes: u64,
    /// The retained byte range `(start, end)` into the full content.
    pub retained_range: (u64, u64),
}

/// `OffloadHandle` — the artifact-addressed offload record (§5c.1 ADR-0075):
/// `content_address`, `media_type`, `size`, `label`, `excerpt_report`,
/// `read_capability` (`Ref` to a `ReadSurface` — a handle without a reachable
/// read capability is `HandleUnreadable`, a typed failure never a silent skip).
#[derive(Debug, Clone)]
pub struct OffloadHandle {
    /// The content address (ART-ID).
    pub content_address: String,
    /// The media type.
    pub media_type: String,
    /// The full size in bytes.
    pub size: u64,
    /// The full content's label.
    pub label: Label,
    /// The excerpt report.
    pub excerpt_report: ExcerptReport,
    /// `Ref` to a `ReadSurface` the model may call to expand.
    pub read_capability: String,
}

impl OffloadHandle {
    /// Canonical JSON.
    pub fn to_json(&self) -> Json {
        Json::obj([
            ("content_address", Json::str(self.content_address.clone())),
            ("media_type", Json::str(self.media_type.clone())),
            ("size", Json::Int(self.size as i64)),
            ("label", crate::codec::label_json(&self.label)),
            (
                "excerpt_report",
                Json::obj([
                    (
                        "truncated_by",
                        self.excerpt_report
                            .truncated_by
                            .as_ref()
                            .map(|t| Json::str(t.clone()))
                            .unwrap_or(Json::Null),
                    ),
                    (
                        "total_lines",
                        Json::Int(self.excerpt_report.total_lines as i64),
                    ),
                    (
                        "total_bytes",
                        Json::Int(self.excerpt_report.total_bytes as i64),
                    ),
                    (
                        "retained_range",
                        Json::obj([
                            (
                                "start",
                                Json::Int(self.excerpt_report.retained_range.0 as i64),
                            ),
                            (
                                "end",
                                Json::Int(self.excerpt_report.retained_range.1 as i64),
                            ),
                        ]),
                    ),
                ]),
            ),
            ("read_capability", Json::str(self.read_capability.clone())),
        ])
    }
}

/// The offload failures — `HandleUnreadable` when the handle's
/// `read_capability` names no reachable `ReadSurface`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OffloadError {
    /// The handle's read capability is empty/unreachable.
    HandleUnreadable {
        /// The content address.
        content_address: String,
    },
}

/// `excerpt(content, policy) -> (excerpted_text, report)` — the deterministic
/// excerpt (§5c.1): line- or byte-bounded per `mode`, token-bounded last
/// (`truncated_by` records the tightest bound hit; an exact fit carries
/// `None`). Token counting is delegated — the caller passes a `&dyn Fn(&str)
/// -> u64` estimator so the excerpt stays estimator-pinned (I-DET).
pub fn excerpt(
    content: &str,
    policy: &ExcerptPolicy,
    estimate: &dyn Fn(&str) -> u64,
) -> (String, ExcerptReport) {
    let total_bytes = content.len() as u64;
    let total_lines = content.lines().count() as u64;
    let mut out = content.to_string();
    let mut truncated_by: Option<String> = None;

    if let Some(max_lines) = policy.max_lines {
        if total_lines > max_lines {
            truncated_by = Some("lines".to_string());
            out = match policy.mode {
                ExcerptMode::Head => content
                    .lines()
                    .take(max_lines as usize)
                    .collect::<Vec<_>>()
                    .join("\n"),
                ExcerptMode::Tail => content
                    .lines()
                    .skip((total_lines - max_lines) as usize)
                    .collect::<Vec<_>>()
                    .join("\n"),
                ExcerptMode::HeadTail => {
                    let head = max_lines / 2;
                    let tail = max_lines - head;
                    let mut lines: Vec<&str> = content.lines().take(head as usize).collect();
                    lines.extend(content.lines().skip((total_lines - tail) as usize));
                    lines.join("\n")
                }
            };
        }
    }
    if let Some(max_bytes) = policy.max_bytes {
        if out.len() as u64 > max_bytes {
            truncated_by = Some("bytes".to_string());
            let mut end = max_bytes as usize;
            while end > 0 && !out.is_char_boundary(end) {
                end -= 1;
            }
            out.truncate(end);
        }
    }
    if estimate(&out) > policy.max_tokens {
        truncated_by = Some("tokens".to_string());
        // Binary-search the largest prefix under the token bound — honest
        // under any estimator (never assumes a chars-per-token ratio).
        let mut lo = 0usize;
        let mut hi = out.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let mid = (0..=mid)
                .rev()
                .find(|m| out.is_char_boundary(*m))
                .unwrap_or(0);
            if estimate(&out[..mid]) > policy.max_tokens {
                hi = mid.saturating_sub(1);
            } else {
                lo = (mid..=out.len())
                    .find(|m| out.is_char_boundary(*m))
                    .unwrap_or(out.len());
                if estimate(&out[..lo]) > policy.max_tokens {
                    hi = mid.saturating_sub(1);
                } else {
                    lo = mid + 1;
                }
            }
        }
        let mut end = lo.min(out.len());
        while end > 0 && !out.is_char_boundary(end) {
            end -= 1;
        }
        out.truncate(end);
    }
    let retained_end = match policy.mode {
        ExcerptMode::Tail => total_bytes,
        _ => out.len() as u64,
    };
    let retained_start = match policy.mode {
        ExcerptMode::Tail => total_bytes.saturating_sub(out.len() as u64),
        _ => 0,
    };
    (
        out,
        ExcerptReport {
            truncated_by,
            total_lines,
            total_bytes,
            retained_range: (retained_start, retained_end),
        },
    )
}

/// `offload(content, media_type, policy, label, read_capability)` — the
/// mandatory path for content a slot cannot carry inline (§5c.1): the full
/// body is content-addressed, the slot's `ExcerptPolicy` produces the index
/// form, and the handle carries `read_capability`. `HandleUnreadable` on an
/// empty capability — the caller resolves the `ReadSurface` ref.
pub fn offload(
    content: &str,
    media_type: &str,
    policy: &ExcerptPolicy,
    label: Label,
    read_capability: &str,
    estimate: &dyn Fn(&str) -> u64,
) -> Result<(String, OffloadHandle), OffloadError> {
    let content_address = idp_id(OFFLOAD_IDP, content.as_bytes());
    if read_capability.is_empty() {
        return Err(OffloadError::HandleUnreadable { content_address });
    }
    let (excerpted, report) = excerpt(content, policy, estimate);
    Ok((
        excerpted,
        OffloadHandle {
            content_address,
            media_type: media_type.to_string(),
            size: content.len() as u64,
            label,
            excerpt_report: report,
            read_capability: read_capability.to_string(),
        },
    ))
}
